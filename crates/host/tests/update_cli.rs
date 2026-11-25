//! `--update-keygen` and `--update-sign` (spec 032, R7): the key file, the signed update file the
//! core accepts, and every refusal (exit 2, nothing written). Keys are made here, never read from
//! the repository.

use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use solite_core::update::{UpdateAdapter, Updates};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "solite-update-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn host(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args(args)
        .output()
        .unwrap()
}

fn keygen(file: &Path) -> Output {
    host(&["--update-keygen".as_ref(), file.as_os_str()])
}

fn sign(image: &Path, version: &str, key: &Path, out: &Path) -> Output {
    host(&[
        "--update-sign".as_ref(),
        image.as_os_str(),
        "--version".as_ref(),
        version.as_ref(),
        "--key".as_ref(),
        key.as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ])
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A key file made by `--update-keygen`, and the public key it printed.
fn key(dir: &Path) -> (PathBuf, String) {
    let file = dir.join("signing.update-key");
    let output = keygen(&file);
    assert!(output.status.success(), "{output:?}");
    let public = stdout(&output)
        .lines()
        .find_map(|l| l.strip_prefix("public key: ").map(str::to_string))
        .expect("the public key line");
    (file, public)
}

fn image_file(dir: &Path, bytes: &[u8]) -> PathBuf {
    let file = dir.join("image.bin");
    std::fs::write(&file, bytes).unwrap();
    file
}

/// An ESP32 application image: `0xE9`, the descriptor's magic at 32, its version at 48.
fn esp_image(version: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; 4096];
    bytes[0] = 0xE9;
    bytes[32..36].copy_from_slice(&0xABCD_5432u32.to_le_bytes());
    bytes[48..48 + version.len()].copy_from_slice(version.as_bytes());
    bytes
}

/// An adapter that keeps the bytes in memory.
#[derive(Default)]
struct Memory {
    slot: Vec<u8>,
    finished: bool,
}

impl UpdateAdapter for Memory {
    fn capacity(&self) -> u64 {
        0x40_0000
    }
    fn begin(&mut self, _length: u32) -> Result<(), String> {
        Ok(())
    }
    fn write(&mut self, chunk: &[u8]) -> Result<(), String> {
        self.slot.extend_from_slice(chunk);
        Ok(())
    }
    fn abort(&mut self) {
        self.slot.clear();
    }
    fn finish(&mut self) -> Result<(), String> {
        self.finished = true;
        Ok(())
    }
    fn pending(&self) -> bool {
        false
    }
    fn confirm(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn roll_back(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn keygen_writes_a_0600_key_file_and_prints_the_public_key() {
    let dir = temp();
    let (file, public) = key(&dir);
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.len(), 65);
    assert!(text.ends_with('\n') && text[..64].bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(public.len(), 64);
    assert_ne!(text.trim(), public, "the public key is not the seed");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_file_signed_with_the_generated_key_is_accepted_by_the_core_with_the_printed_public_key() {
    let dir = temp();
    let (key_file, public) = key(&dir);
    let image: Vec<u8> = (0..30_000u32)
        .map(|i| u8::try_from(i % 253).unwrap())
        .collect();
    let out = dir.join("image.sup");
    let output = sign(&image_file(&dir, &image), "0.2.0", &key_file, &out);
    assert!(output.status.success(), "{output:?}");
    let text = stdout(&output);
    assert!(
        text.contains(&format!(
            "signed: {}, version 0.2.0, 30000 bytes, key {}",
            out.display(),
            &public[..8]
        )),
        "{text}"
    );
    assert!(text.contains("signed: no application descriptor, version not checked"));
    let (updates, _) = Updates::from_key_text(Some(&public));
    let mut adapter = Memory::default();
    let response = updates.install(&mut Cursor::new(std::fs::read(&out).unwrap()), &mut adapter);
    assert_eq!(response.status, 200, "{:?}", response.body);
    assert_eq!(adapter.slot, image);
    assert!(adapter.finished);
}

#[test]
fn keygen_refuses_an_existing_file_and_a_folder_inside_a_git_working_tree() {
    let dir = temp();
    let existing = dir.join("existing.update-key");
    std::fs::write(&existing, "keep me").unwrap();
    let output = keygen(&existing);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr(&output).contains("exists"));
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "keep me");

    let repository = dir.join("repository");
    std::fs::create_dir_all(repository.join(".git")).unwrap();
    std::fs::create_dir_all(repository.join("deep/er")).unwrap();
    for folder in [repository.clone(), repository.join("deep/er")] {
        let file = folder.join("signing.update-key");
        let output = keygen(&file);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(
            stderr(&output).contains("git working tree"),
            "{}",
            stderr(&output)
        );
        assert!(!file.exists());
        assert!(!stdout(&output).contains("public key"));
    }
}

#[test]
fn sign_refuses_what_it_cannot_sign_and_writes_nothing() {
    let dir = temp();
    let (key_file, _) = key(&dir);
    let out = dir.join("refused.sup");
    let image = image_file(&dir, &[1u8; 500]);
    let short_key = dir.join("short.update-key");
    std::fs::write(&short_key, "0123456789\n").unwrap();
    // `from_str_radix` accepts a `+` sign: a pair like `+1` is not hexadecimal.
    let plus_key = dir.join("plus.update-key");
    std::fs::write(&plus_key, format!("{}\n", "+1".repeat(32))).unwrap();
    let empty = dir.join("empty.bin");
    std::fs::write(&empty, b"").unwrap();
    let big = dir.join("big.bin");
    std::fs::write(&big, vec![0u8; 0x40_0000 + 1]).unwrap();
    let taken = dir.join("taken.sup");
    std::fs::write(&taken, "mine").unwrap();
    let cases = [
        ("empty image", sign(&empty, "0.2.0", &key_file, &out)),
        ("image over 4 MB", sign(&big, "0.2.0", &key_file, &out)),
        ("version 1.2", sign(&image, "1.2", &key_file, &out)),
        (
            "version 70000.0.0",
            sign(&image, "70000.0.0", &key_file, &out),
        ),
        ("a short key file", sign(&image, "0.2.0", &short_key, &out)),
        ("a key with a +", sign(&image, "0.2.0", &plus_key, &out)),
        ("an existing out", sign(&image, "0.2.0", &key_file, &taken)),
    ];
    for (what, output) in cases {
        assert_eq!(output.status.code(), Some(2), "{what}: {output:?}");
        assert!(!stderr(&output).is_empty(), "{what}");
    }
    assert!(!out.exists());
    assert_eq!(std::fs::read_to_string(&taken).unwrap(), "mine");
}

#[test]
fn the_version_is_tied_to_the_application_descriptor_of_an_esp32_image() {
    let dir = temp();
    let (key_file, _) = key(&dir);
    let image = image_file(&dir, &esp_image("0.2.0"));
    let wrong = dir.join("wrong.sup");
    let output = sign(&image, "0.3.0", &key_file, &wrong);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr(&output).contains("0.2.0"), "{}", stderr(&output));
    assert!(!wrong.exists());
    let right = dir.join("right.sup");
    let output = sign(&image, "0.2.0", &key_file, &right);
    assert!(output.status.success(), "{output:?}");
    assert!(right.exists());
    assert!(!stdout(&output).contains("no application descriptor"));
}

#[test]
fn the_update_tools_refuse_to_be_mixed_with_the_radio_options_or_to_be_incomplete() {
    let dir = temp();
    let (key_file, _) = key(&dir);
    let image = image_file(&dir, &[1u8; 10]);
    let out = dir.join("x.sup");
    for args in [
        vec!["--update-sign".as_ref(), image.as_os_str()],
        vec![
            "--update-keygen".as_ref(),
            out.as_os_str(),
            "--listen".as_ref(),
        ],
        vec!["--version".as_ref(), "0.2.0".as_ref()],
        vec![
            "--update-sign".as_ref(),
            image.as_os_str(),
            "--version".as_ref(),
            "0.2.0".as_ref(),
            "--key".as_ref(),
            key_file.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
            "--serve".as_ref(),
        ],
    ] {
        let output = host(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(stderr(&output).starts_with("usage:"));
    }
    assert!(!out.exists());
}
