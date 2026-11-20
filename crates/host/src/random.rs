//! Random bytes from the operating system: the core has no random source of its own.

use std::fs::File;
use std::io::Read;

const SOURCE: &str = "/dev/urandom";

/// `/dev/urandom`, opened once.
pub struct SystemRandom {
    file: File,
}

impl SystemRandom {
    pub fn open() -> Result<Self, String> {
        File::open(SOURCE)
            .map(|file| SystemRandom { file })
            .map_err(|e| format!("cannot read {SOURCE}: {e}"))
    }

    /// Fills the buffer with random bytes.
    pub fn fill(&mut self, buffer: &mut [u8]) -> Result<(), String> {
        self.file
            .read_exact(buffer)
            .map_err(|e| format!("cannot read {SOURCE}: {e}"))
    }
}
