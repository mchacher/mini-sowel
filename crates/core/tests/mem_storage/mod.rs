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

    pub fn text(&self, name: &str) -> Option<String> {
        self.files.borrow().get(name).cloned()
    }
}

impl Storage for &MemStorage {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
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
        let what = name.trim_end_matches(".json");
        let mut text = String::new();
        while let Some(chunk) = next().map_err(|e| format!("{what}: cannot write: {e}"))? {
            text.push_str(&chunk);
        }
        if self.fail_writes.get() {
            return Err(format!("{what}: memory://{name}: cannot write: full"));
        }
        self.files.borrow_mut().insert(name.to_string(), text);
        Ok(())
    }

    fn locate(&self, name: &str) -> String {
        format!("memory://{name}")
    }
}
