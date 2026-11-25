//! A fake update adapter (spec 032): the inactive slot in memory, the boot record as two flags,
//! the calls it received, and switches to fail a write, a `finish`, a `confirm` or a `roll_back`.
//! In counting mode (`keep: false`) it keeps no byte, for the memory test.
#![allow(dead_code)]

use solite_core::update::UpdateAdapter;

pub struct FakeAdapter {
    pub capacity: u64,
    /// False: the bytes are counted, not kept.
    pub keep: bool,
    pub slot: Vec<u8>,
    pub written: usize,
    pub max_write: usize,
    /// The names of the calls, in order.
    pub calls: Vec<&'static str>,
    /// The boot record: true once `finish` made the inactive slot the next boot.
    pub next_boot_is_inactive: bool,
    pub pending: bool,
    /// Writes fail once this many bytes were written.
    pub fail_writes_after: Option<usize>,
    pub fail_finish: bool,
    /// How many `confirm` and `roll_back` calls still fail.
    pub confirm_failures: u32,
    pub roll_back_failures: u32,
}

impl FakeAdapter {
    pub fn new() -> Self {
        FakeAdapter {
            capacity: 4 * 1024 * 1024,
            keep: true,
            slot: Vec::new(),
            written: 0,
            max_write: 0,
            calls: Vec::new(),
            next_boot_is_inactive: false,
            pending: false,
            fail_writes_after: None,
            fail_finish: false,
            confirm_failures: 0,
            roll_back_failures: 0,
        }
    }

    pub fn count(&self, call: &str) -> usize {
        self.calls.iter().filter(|c| **c == call).count()
    }
}

impl UpdateAdapter for FakeAdapter {
    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn begin(&mut self, _length: u32) -> Result<(), String> {
        self.calls.push("begin");
        self.slot.clear();
        self.written = 0;
        Ok(())
    }

    fn write(&mut self, chunk: &[u8]) -> Result<(), String> {
        self.calls.push("write");
        if self.fail_writes_after.is_some_and(|n| self.written >= n) {
            return Err("flash write refused".into());
        }
        self.max_write = self.max_write.max(chunk.len());
        self.written += chunk.len();
        if self.keep {
            self.slot.extend_from_slice(chunk);
        }
        Ok(())
    }

    fn abort(&mut self) {
        self.calls.push("abort");
        self.slot.clear();
        self.written = 0;
    }

    fn finish(&mut self) -> Result<(), String> {
        self.calls.push("finish");
        if self.fail_finish {
            return Err("image refused by the platform".into());
        }
        self.next_boot_is_inactive = true;
        self.pending = true;
        Ok(())
    }

    fn pending(&self) -> bool {
        self.pending
    }

    fn confirm(&mut self) -> Result<(), String> {
        self.calls.push("confirm");
        if self.confirm_failures > 0 {
            self.confirm_failures -= 1;
            return Err("flash error".into());
        }
        self.pending = false;
        Ok(())
    }

    fn roll_back(&mut self) -> Result<(), String> {
        self.calls.push("roll_back");
        if self.roll_back_failures > 0 {
            self.roll_back_failures -= 1;
            return Err("flash error".into());
        }
        self.pending = false;
        self.next_boot_is_inactive = false;
        Ok(())
    }
}
