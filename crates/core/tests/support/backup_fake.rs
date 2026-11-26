//! The full backup's files in memory (spec 035), for the tests: the data folder as a map of names
//! to bytes (staged copies, marker and leftovers included), a log of the calls, failures on demand
//! and a fake clock for the pacing.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::rc::Rc;

use solite_core::backup::{BackupFiles, BackupState, Pacing, StagedFile};
use solite_core::engine::Storage;

pub const MARKER: &str = "restore.pending";
pub const MARK: &str = "zigbee.restore";

#[derive(Default)]
pub struct Fs {
    pub files: BTreeMap<String, Vec<u8>>,
    /// The calls, in order: `stage:<name>`, `finish:<name>`, `commit`, `discard`, `promote:<name>`...
    pub log: Vec<String>,
    /// The state's phase seen by `commit`.
    pub phase_at_commit: Option<solite_core::backup::Phase>,
}

/// A data folder in memory; clones share it, as two handles on one folder.
#[derive(Clone)]
pub struct MemFiles {
    pub fs: Rc<RefCell<Fs>>,
    pub wifi: bool,
    pub restarts: bool,
    /// Reading this file fails after this many bytes.
    pub fail_read: Rc<RefCell<Option<(String, usize)>>>,
    /// `promote` fails after this many successful ones.
    pub promotes_left: Rc<Cell<Option<usize>>>,
    /// The state `commit` looks at, to see the order of the calls.
    pub probe: Option<std::sync::Arc<BackupState>>,
}

impl MemFiles {
    pub fn new() -> Self {
        MemFiles {
            fs: Rc::default(),
            wifi: true,
            restarts: true,
            fail_read: Rc::default(),
            promotes_left: Rc::default(),
            probe: None,
        }
    }

    pub fn with(files: &[(&str, &str)]) -> Self {
        let this = MemFiles::new();
        for (name, text) in files {
            this.put(name, text.as_bytes());
        }
        this
    }

    pub fn put(&self, name: &str, bytes: &[u8]) {
        self.fs
            .borrow_mut()
            .files
            .insert(name.to_string(), bytes.to_vec());
    }

    pub fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.fs.borrow().files.get(name).cloned()
    }

    pub fn text(&self, name: &str) -> Option<String> {
        self.get(name).map(|b| String::from_utf8(b).unwrap())
    }

    pub fn has(&self, name: &str) -> bool {
        self.fs.borrow().files.contains_key(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.fs.borrow().files.keys().cloned().collect()
    }

    pub fn log(&self) -> Vec<String> {
        self.fs.borrow().log.clone()
    }

    /// The staged copies and the marker: nothing of a restore is left.
    pub fn nothing_staged(&self) -> bool {
        self.fs
            .borrow()
            .files
            .keys()
            .all(|n| !n.ends_with(".json.restore") && n != MARKER)
    }

    /// Every file, to compare a folder before and after.
    pub fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        self.fs.borrow().files.clone()
    }

    fn note(&self, line: String) {
        self.fs.borrow_mut().log.push(line);
    }
}

/// A reader that gives its bytes and then fails, for an unreadable file.
struct Flaky {
    data: Cursor<Vec<u8>>,
    fail_after: usize,
    given: usize,
}

impl Read for Flaky {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.given >= self.fail_after {
            return Err(io::Error::other("unreadable"));
        }
        let room = (self.fail_after - self.given).min(out.len());
        let count = self.data.read(&mut out[..room])?;
        self.given += count;
        Ok(count)
    }
}

struct Staged {
    fs: Rc<RefCell<Fs>>,
    key: String,
    name: String,
}

impl Write for Staged {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.fs
            .borrow_mut()
            .files
            .entry(self.key.clone())
            .or_default()
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl StagedFile for Staged {
    fn finish(self: Box<Self>) -> Result<(), String> {
        self.fs
            .borrow_mut()
            .log
            .push(format!("finish:{}", self.name));
        Ok(())
    }
}

/// `zigbee.json` and the label: `zigbee-<label>.json`.
fn aside_name(name: &str, label: &str) -> String {
    let stem = name.trim_end_matches(".json");
    format!("{stem}-{label}.json")
}

impl BackupFiles for MemFiles {
    fn has_wifi(&self) -> bool {
        self.wifi
    }

    fn restarts(&self) -> bool {
        self.restarts
    }

    fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        let bytes = self.get(name).or_else(|| self.get(&format!("{name}.new")));
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        if let Some((file, after)) = self.fail_read.borrow().as_ref()
            && file == name
        {
            return Ok(Some(Box::new(Flaky {
                data: Cursor::new(bytes),
                fail_after: *after,
                given: 0,
            })));
        }
        Ok(Some(Box::new(Cursor::new(bytes))))
    }

    fn stage(&mut self, name: &str) -> Result<Box<dyn StagedFile>, String> {
        let key = format!("{name}.restore");
        self.note(format!("stage:{name}"));
        self.fs.borrow_mut().files.insert(key.clone(), Vec::new());
        Ok(Box::new(Staged {
            fs: Rc::clone(&self.fs),
            key,
            name: name.to_string(),
        }))
    }

    fn open_staged(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        Ok(self
            .get(&format!("{name}.restore"))
            .map(|b| Box::new(Cursor::new(b)) as Box<dyn Read>))
    }

    fn discard(&mut self) -> Result<(), String> {
        self.note("discard".to_string());
        self.fs
            .borrow_mut()
            .files
            .retain(|n, _| !n.ends_with(".json.restore") && n != MARKER);
        Ok(())
    }

    fn commit(&mut self, marker: &str) -> Result<(), String> {
        self.note("commit".to_string());
        self.fs.borrow_mut().phase_at_commit = self.probe.as_ref().map(|p| p.phase());
        self.put(MARKER, marker.as_bytes());
        Ok(())
    }

    fn pending(&mut self) -> Result<Option<String>, String> {
        Ok(self.text(MARKER))
    }

    fn promote(&mut self, name: &str) -> Result<(), String> {
        if let Some(left) = self.promotes_left.get() {
            if left == 0 {
                return Err("power cut".to_string());
            }
            self.promotes_left.set(Some(left - 1));
        }
        self.note(format!("promote:{name}"));
        let mut fs = self.fs.borrow_mut();
        fs.files.remove(&format!("{name}.new"));
        fs.files.remove(&format!("{name}.tmp"));
        if let Some(bytes) = fs.files.remove(&format!("{name}.restore")) {
            fs.files.insert(name.to_string(), bytes);
        }
        Ok(())
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        self.note(format!("remove:{name}"));
        let mut fs = self.fs.borrow_mut();
        for key in [
            name.to_string(),
            format!("{name}.new"),
            format!("{name}.tmp"),
        ] {
            fs.files.remove(&key);
        }
        Ok(())
    }

    fn set_aside(&mut self, name: &str, label: &str) -> Result<(), String> {
        self.note(format!("aside:{name}"));
        let mut fs = self.fs.borrow_mut();
        if let Some(bytes) = fs.files.remove(name) {
            fs.files.insert(aside_name(name, label), bytes);
        }
        Ok(())
    }

    fn clear_pending(&mut self) -> Result<(), String> {
        self.fs.borrow_mut().files.remove(MARKER);
        Ok(())
    }

    fn restore_mark(&mut self) -> Result<bool, String> {
        Ok(self.has(MARK))
    }

    fn set_restore_mark(&mut self) -> Result<(), String> {
        self.note("mark".to_string());
        self.put(MARK, b"1");
        Ok(())
    }

    fn clear_restore_mark(&mut self) -> Result<(), String> {
        self.fs.borrow_mut().files.remove(MARK);
        Ok(())
    }
}

/// The engine's storage over the same folder: a missing file reads its `.new`, as the box's.
impl Storage for &MemFiles {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        Ok(self
            .get(name)
            .or_else(|| self.get(&format!("{name}.new")))
            .map(|b| String::from_utf8(b).unwrap()))
    }

    fn write(
        &self,
        _name: &str,
        _next: &mut dyn FnMut() -> io::Result<Option<String>>,
    ) -> Result<(), String> {
        Err("not used".to_string())
    }

    fn locate(&self, name: &str) -> String {
        format!("memory://{name}")
    }
}

/// A fake clock: each pause moves it by `step_ms`; a hook runs at each pause (to clear a mark).
pub struct Clock {
    pub now: Rc<Cell<u64>>,
    pub pauses: Rc<Cell<u32>>,
    pub step_ms: u64,
    pub hook: Option<Box<dyn FnMut(u32)>>,
}

impl Clock {
    pub fn new() -> Self {
        Clock {
            now: Rc::default(),
            pauses: Rc::default(),
            step_ms: 10,
            hook: None,
        }
    }
}

impl Pacing for Clock {
    fn now_ms(&mut self) -> u64 {
        self.now.get()
    }

    fn pause(&mut self) {
        self.pauses.set(self.pauses.get() + 1);
        self.now.set(self.now.get() + self.step_ms);
        if let Some(hook) = self.hook.as_mut() {
            hook(self.pauses.get());
        }
    }

    fn wait_ms(&mut self, ms: u64) {
        self.now.set(self.now.get() + ms);
    }
}
