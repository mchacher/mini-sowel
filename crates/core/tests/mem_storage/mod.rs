//! The engine's files in memory, for the tests: what was written, and failures on demand.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;

use solite_core::engine::Storage;

#[derive(Default)]
pub struct MemStorage {
    pub files: RefCell<BTreeMap<String, String>>,
    /// Every write fails while this is set (the file is left as it was).
    pub fail_writes: Cell<bool>,
    /// Reading this file fails.
    pub unreadable: RefCell<Option<String>>,
    /// Writes attempted, failed ones included.
    pub attempts: Cell<usize>,
    /// Writes attempted per file, failed ones included.
    pub per_file: RefCell<BTreeMap<String, usize>>,
    /// Writes of this file fail while it is set.
    pub fail_file: RefCell<Option<String>>,
    /// The binary files (spec 042): the installed script recipes, `scripts/<id>.srs`.
    pub bytes: RefCell<BTreeMap<String, Vec<u8>>>,
    /// Writes of a binary file fail while this is set (the file is left as it was).
    pub fail_byte_writes: Cell<bool>,
    /// Removals fail while this is set.
    pub fail_removals: Cell<bool>,
    /// Binary writes attempted per file, failed ones included.
    pub byte_writes: RefCell<BTreeMap<String, usize>>,
    /// Removals attempted, failed ones included.
    pub removals: Cell<usize>,
    /// Files read, of any kind (a `GET` must read none).
    pub reads: Cell<usize>,
}

impl MemStorage {
    pub fn with(files: &[(&str, &str)]) -> Self {
        let storage = MemStorage::default();
        for (name, text) in files {
            storage
                .files
                .borrow_mut()
                .insert((*name).to_string(), (*text).to_string());
        }
        storage
    }

    /// The writes attempted of one file.
    pub fn writes(&self, name: &str) -> usize {
        self.per_file.borrow().get(name).copied().unwrap_or(0)
    }

    pub fn text(&self, name: &str) -> Option<String> {
        self.files.borrow().get(name).cloned()
    }

    /// A binary file, as written.
    pub fn put(&self, name: &str, bytes: &[u8]) {
        self.bytes
            .borrow_mut()
            .insert(name.to_string(), bytes.to_vec());
    }

    pub fn file(&self, name: &str) -> Option<Vec<u8>> {
        self.bytes.borrow().get(name).cloned()
    }

    /// The binary writes attempted of one file.
    pub fn byte_writes_of(&self, name: &str) -> usize {
        self.byte_writes.borrow().get(name).copied().unwrap_or(0)
    }

    /// A save cut between the delete of the old file and the rename of the new: only `<name>.new`
    /// is left.
    pub fn cut_save(&self, name: &str) {
        let mut bytes = self.bytes.borrow_mut();
        if let Some(content) = bytes.remove(name) {
            bytes.insert(format!("{name}.new"), content);
        }
    }
}

impl Storage for &MemStorage {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        self.reads.set(self.reads.get() + 1);
        if self.unreadable.borrow().as_deref() == Some(name) {
            return Err(format!("{name}: memory://{name}: cannot read: boom"));
        }
        Ok(self.text(name))
    }

    fn write(
        &self,
        name: &str,
        next: &mut dyn FnMut() -> io::Result<Option<String>>,
    ) -> Result<(), String> {
        self.attempts.set(self.attempts.get() + 1);
        *self
            .per_file
            .borrow_mut()
            .entry(name.to_string())
            .or_default() += 1;
        let what = name.trim_end_matches(".json");
        let mut text = String::new();
        while let Some(chunk) = next().map_err(|e| format!("{what}: cannot write: {e}"))? {
            text.push_str(&chunk);
        }
        if self.fail_writes.get() || self.fail_file.borrow().as_deref() == Some(name) {
            return Err(format!("{what}: memory://{name}: cannot write: full"));
        }
        self.files.borrow_mut().insert(name.to_string(), text);
        Ok(())
    }

    fn locate(&self, name: &str) -> String {
        format!("memory://{name}")
    }

    fn read_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.file(name))
    }

    fn write_bytes(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        *self
            .byte_writes
            .borrow_mut()
            .entry(name.to_string())
            .or_default() += 1;
        if self.fail_byte_writes.get() {
            return Err(format!("memory://{name}: cannot write: full"));
        }
        self.put(name, bytes);
        Ok(())
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        self.removals.set(self.removals.get() + 1);
        if self.fail_removals.get() {
            return Err(format!("memory://{name}: cannot remove: locked"));
        }
        self.bytes.borrow_mut().remove(name);
        Ok(())
    }

    fn list(&self, folder: &str) -> Result<Vec<String>, String> {
        let prefix = format!("{folder}/");
        let mut bytes = self.bytes.borrow_mut();
        // A lone `<name>.new` is promoted, as the real storages do; a `.tmp` is ignored.
        let news: Vec<String> = bytes
            .keys()
            .filter(|key| key.starts_with(&prefix) && key.ends_with(".new"))
            .cloned()
            .collect();
        for new in news {
            let base = new.trim_end_matches(".new").to_string();
            let content = bytes.remove(&new).unwrap();
            bytes.entry(base).or_insert(content);
        }
        Ok(bytes
            .keys()
            .filter(|key| key.starts_with(&prefix) && !key.ends_with(".tmp"))
            .map(|key| key[prefix.len()..].to_string())
            .collect())
    }

    fn usage(&self) -> Result<(u64, Option<u64>), String> {
        let text: usize = self.files.borrow().values().map(String::len).sum();
        let bytes: usize = self.bytes.borrow().values().map(Vec::len).sum();
        Ok(((text + bytes) as u64, Some(1_048_576)))
    }
}
