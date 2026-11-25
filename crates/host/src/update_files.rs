//! The update adapter of the Mac (spec 032, R7): the two slots as files in `<data>/update/`
//! (`slot-0.bin`, `slot-1.bin`) and the boot record `boot.json` (`{"boot": 0|1, "pending": bool}`,
//! absent = slot 0, not pending). The host stores the image, it does not run it nor restart.
//! Every boot record write goes through a temporary file and a rename.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use solite_core::devices::lock;
use solite_core::update::{SLOT_BYTES, UpdateAdapter, Updates};

/// The boot record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Boot {
    slot: u8,
    pending: bool,
}

struct Inner {
    dir: PathBuf,
    /// The inactive slot's file while an image is written.
    part: Option<File>,
}

/// The files of `<data>/update/`. Cloning gives another handle on the same files: each call locks
/// for its own duration only, so a second installation is refused by the core (409), never held up.
#[derive(Clone)]
pub struct FileAdapter {
    inner: Arc<Mutex<Inner>>,
}

impl Inner {
    fn boot(&self) -> Boot {
        let default = Boot {
            slot: 0,
            pending: false,
        };
        let Ok(text) = fs::read_to_string(self.dir.join("boot.json")) else {
            return default;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return default;
        };
        Boot {
            slot: u8::from(value["boot"].as_u64() == Some(1)),
            pending: value["pending"].as_bool().unwrap_or(false),
        }
    }

    fn write_boot(&self, boot: Boot) -> Result<(), String> {
        let text = format!("{{\"boot\":{},\"pending\":{}}}\n", boot.slot, boot.pending);
        let temporary = self.dir.join("boot.json.tmp");
        (|| -> std::io::Result<()> {
            fs::create_dir_all(&self.dir)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, self.dir.join("boot.json"))
        })()
        .map_err(|e| format!("cannot write boot.json: {e}"))
    }

    fn slot_path(&self, slot: u8) -> PathBuf {
        self.dir.join(format!("slot-{slot}.bin"))
    }

    fn part_path(&self, slot: u8) -> PathBuf {
        self.dir.join(format!("slot-{slot}.bin.part"))
    }
}

impl FileAdapter {
    pub fn new(data: &Path) -> Self {
        FileAdapter {
            inner: Arc::new(Mutex::new(Inner {
                dir: data.join("update"),
                part: None,
            })),
        }
    }

    /// The slot the boot record points to.
    pub fn boot_slot(&self) -> u8 {
        lock(&self.inner).boot().slot
    }
}

impl UpdateAdapter for FileAdapter {
    fn capacity(&self) -> u64 {
        SLOT_BYTES
    }

    fn begin(&mut self, _length: u32) -> Result<(), String> {
        let mut inner = lock(&self.inner);
        fs::create_dir_all(&inner.dir)
            .map_err(|e| format!("cannot create the update folder: {e}"))?;
        let path = inner.part_path(1 - inner.boot().slot);
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        inner.part = Some(file);
        Ok(())
    }

    fn write(&mut self, chunk: &[u8]) -> Result<(), String> {
        let mut inner = lock(&self.inner);
        let file = inner.part.as_mut().ok_or("no slot is open")?;
        file.write_all(chunk)
            .map_err(|e| format!("cannot write the slot: {e}"))
    }

    fn abort(&mut self) {
        let mut inner = lock(&self.inner);
        inner.part = None;
        let _ = fs::remove_file(inner.part_path(1 - inner.boot().slot));
    }

    fn finish(&mut self) -> Result<(), String> {
        let mut inner = lock(&self.inner);
        let slot = 1 - inner.boot().slot;
        let file = inner.part.take().ok_or("no slot is open")?;
        file.sync_all()
            .map_err(|e| format!("cannot write the slot: {e}"))?;
        drop(file);
        fs::rename(inner.part_path(slot), inner.slot_path(slot))
            .map_err(|e| format!("cannot close the slot: {e}"))?;
        inner.write_boot(Boot {
            slot,
            pending: true,
        })
    }

    fn pending(&self) -> bool {
        lock(&self.inner).boot().pending
    }

    fn confirm(&mut self) -> Result<(), String> {
        let inner = lock(&self.inner);
        let boot = inner.boot();
        inner.write_boot(Boot {
            slot: boot.slot,
            pending: false,
        })
    }

    fn roll_back(&mut self) -> Result<(), String> {
        let inner = lock(&self.inner);
        let boot = inner.boot();
        inner.write_boot(Boot {
            slot: 1 - boot.slot,
            pending: false,
        })
    }
}

/// The updates of this build, from its key text (`option_env!("SOLITE_UPDATE_PUBLIC_KEY")`), and
/// the lines to print at start: the key's line (`update: no key, updates refused`, `update:
/// invalid key, updates refused` or `update: key <8 hex>`), then `update: slot <n> on probation`
/// when the boot record says the image booted is not confirmed yet.
pub fn start_updates(key: Option<&str>, adapter: &FileAdapter) -> (Updates, Vec<String>) {
    let (updates, line) = Updates::from_key_text(key);
    let mut lines = vec![line];
    if adapter.pending() {
        lines.push(format!("update: slot {} on probation", adapter.boot_slot()));
    }
    (updates, lines)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn data() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "solite-update-files-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The public key of a pair made now from random bytes.
    fn public_key() -> String {
        let mut seed = [0u8; 32];
        crate::random::SystemRandom::open()
            .unwrap()
            .fill(&mut seed)
            .unwrap();
        let public = ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes();
        public.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn boot_json(data: &Path) -> String {
        fs::read_to_string(data.join("update/boot.json")).unwrap()
    }

    fn install(adapter: &mut FileAdapter, bytes: &[u8]) {
        adapter.begin(u32::try_from(bytes.len()).unwrap()).unwrap();
        for chunk in bytes.chunks(1000) {
            adapter.write(chunk).unwrap();
        }
        adapter.finish().unwrap();
    }

    #[test]
    fn an_installation_writes_slot_1_and_a_pending_boot_record() {
        let data = data();
        let mut adapter = FileAdapter::new(&data);
        assert_eq!(adapter.capacity(), 0x40_0000);
        assert!(!adapter.pending());
        let image: Vec<u8> = (0..5000u32)
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();
        install(&mut adapter, &image);
        assert_eq!(fs::read(data.join("update/slot-1.bin")).unwrap(), image);
        assert_eq!(boot_json(&data).trim(), r#"{"boot":1,"pending":true}"#);
        assert!(adapter.pending());
        assert!(!data.join("update/slot-1.bin.part").exists());
    }

    #[test]
    fn an_abort_leaves_no_part_file_and_the_boot_record_unchanged() {
        let data = data();
        let mut adapter = FileAdapter::new(&data);
        adapter.begin(100).unwrap();
        adapter.write(&[1; 50]).unwrap();
        assert!(data.join("update/slot-1.bin.part").exists());
        adapter.abort();
        assert!(!data.join("update/slot-1.bin.part").exists());
        assert!(!data.join("update/slot-1.bin").exists());
        assert!(!data.join("update/boot.json").exists());
        assert!(!adapter.pending());
    }

    #[test]
    fn confirm_keeps_the_slot_and_roll_back_takes_the_other() {
        let data = data();
        let mut adapter = FileAdapter::new(&data);
        install(&mut adapter, &[7; 10]);
        adapter.confirm().unwrap();
        assert_eq!(boot_json(&data).trim(), r#"{"boot":1,"pending":false}"#);
        install(&mut adapter, &[8; 10]);
        assert_eq!(boot_json(&data).trim(), r#"{"boot":0,"pending":true}"#);
        adapter.roll_back().unwrap();
        assert_eq!(boot_json(&data).trim(), r#"{"boot":1,"pending":false}"#);
        assert!(!adapter.pending());
    }

    #[test]
    fn a_second_installation_writes_slot_0_when_booted_on_1() {
        let data = data();
        let mut adapter = FileAdapter::new(&data);
        install(&mut adapter, &[1; 10]);
        adapter.confirm().unwrap();
        assert_eq!(adapter.boot_slot(), 1);
        install(&mut adapter, &[2; 20]);
        assert_eq!(
            fs::read(data.join("update/slot-0.bin")).unwrap(),
            vec![2; 20]
        );
        assert_eq!(
            fs::read(data.join("update/slot-1.bin")).unwrap(),
            vec![1; 10]
        );
    }

    #[test]
    fn the_start_lines_say_no_key_an_invalid_key_or_the_key_and_the_probation() {
        let data = data();
        let adapter = FileAdapter::new(&data);
        let (updates, lines) = start_updates(None, &adapter);
        assert!(!updates.has_key());
        assert_eq!(lines, ["update: no key, updates refused"]);
        let (_, lines) = start_updates(Some(&"zz".repeat(32)), &adapter);
        assert_eq!(lines, ["update: invalid key, updates refused"]);
        let public = public_key();
        let (updates, lines) = start_updates(Some(&public), &adapter);
        assert!(updates.has_key());
        assert_eq!(lines, [format!("update: key {}", &public[..8])]);
    }

    #[test]
    fn a_pending_boot_record_at_start_puts_the_host_on_probation_and_refuses_installations() {
        let data = data();
        fs::create_dir_all(data.join("update")).unwrap();
        fs::write(
            data.join("update/boot.json"),
            r#"{"boot":1,"pending":true}"#,
        )
        .unwrap();
        let mut adapter = FileAdapter::new(&data);
        let (updates, lines) = start_updates(Some(&public_key()), &adapter);
        assert!(adapter.pending());
        assert_eq!(lines[1], "update: slot 1 on probation");
        // The installation is refused for the probation (409), before the file is read.
        let response = updates.install(&mut std::io::empty(), &mut adapter);
        assert_eq!(response.status, 409);
        assert!(response.body.unwrap().contains("not confirmed yet"));
    }
}
