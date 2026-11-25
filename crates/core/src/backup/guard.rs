//! The flash rule of the box, for the backup's files: on the ESP32 a read, a write or an erase
//! of the flash turns the caches off, and ESP-IDF stops the chip (an `assert`, then a restart)
//! when the calling thread's stack is in PSRAM, as the API workers' stacks are. The box runs the
//! backup's jobs on a thread whose stack is in internal RAM; `Guarded` checks it on every call,
//! so that a job started on the wrong thread is an error, never a flash access.

use std::io::{self, Read, Write};

use super::{BackupFiles, StagedFile};

/// The error of a call refused by `Guarded`.
pub const WRONG_STACK: &str =
    "the backup's files are not reachable from this thread (its stack is not in internal RAM)";

/// A `BackupFiles` whose every call, and every read or write of a file it opened, first asks
/// `allowed` (the box: "is the calling thread's stack in internal RAM?"). A refusal is an error,
/// and the inner adapter is not called.
pub struct Guarded<F> {
    inner: F,
    allowed: fn() -> bool,
}

impl<F: BackupFiles> Guarded<F> {
    pub fn new(inner: F, allowed: fn() -> bool) -> Self {
        Guarded { inner, allowed }
    }

    fn check(&self) -> Result<(), String> {
        if (self.allowed)() {
            Ok(())
        } else {
            log::error!("backup: {WRONG_STACK}");
            Err(WRONG_STACK.to_string())
        }
    }
}

fn refused() -> io::Error {
    io::Error::other(WRONG_STACK)
}

/// A file opened through `Guarded`: each read asks again.
struct GuardedRead {
    inner: Box<dyn Read>,
    allowed: fn() -> bool,
}

impl Read for GuardedRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !(self.allowed)() {
            return Err(refused());
        }
        self.inner.read(buffer)
    }
}

/// A staged copy written through `Guarded`: each write, flush and the finish ask again.
struct GuardedStaged {
    inner: Box<dyn StagedFile>,
    allowed: fn() -> bool,
}

impl Write for GuardedStaged {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if !(self.allowed)() {
            return Err(refused());
        }
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        if !(self.allowed)() {
            return Err(refused());
        }
        self.inner.flush()
    }
}

impl StagedFile for GuardedStaged {
    fn finish(self: Box<Self>) -> Result<(), String> {
        if !(self.allowed)() {
            return Err(WRONG_STACK.to_string());
        }
        self.inner.finish()
    }
}

impl<F: BackupFiles> BackupFiles for Guarded<F> {
    fn has_wifi(&self) -> bool {
        self.inner.has_wifi()
    }

    fn restarts(&self) -> bool {
        self.inner.restarts()
    }

    fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        self.check()?;
        let allowed = self.allowed;
        Ok(self
            .inner
            .open(name)?
            .map(|inner| Box::new(GuardedRead { inner, allowed }) as Box<dyn Read>))
    }

    fn stage(&mut self, name: &str) -> Result<Box<dyn StagedFile>, String> {
        self.check()?;
        let inner = self.inner.stage(name)?;
        Ok(Box::new(GuardedStaged {
            inner,
            allowed: self.allowed,
        }))
    }

    fn open_staged(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        self.check()?;
        let allowed = self.allowed;
        Ok(self
            .inner
            .open_staged(name)?
            .map(|inner| Box::new(GuardedRead { inner, allowed }) as Box<dyn Read>))
    }

    fn discard(&mut self) -> Result<(), String> {
        self.check()?;
        self.inner.discard()
    }

    fn commit(&mut self, marker: &str) -> Result<(), String> {
        self.check()?;
        self.inner.commit(marker)
    }

    fn pending(&mut self) -> Result<Option<String>, String> {
        self.check()?;
        self.inner.pending()
    }

    fn promote(&mut self, name: &str) -> Result<(), String> {
        self.check()?;
        self.inner.promote(name)
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        self.check()?;
        self.inner.remove(name)
    }

    fn set_aside(&mut self, name: &str, label: &str) -> Result<(), String> {
        self.check()?;
        self.inner.set_aside(name, label)
    }

    fn clear_pending(&mut self) -> Result<(), String> {
        self.check()?;
        self.inner.clear_pending()
    }

    fn restore_mark(&mut self) -> Result<bool, String> {
        self.check()?;
        self.inner.restore_mark()
    }

    fn set_restore_mark(&mut self) -> Result<(), String> {
        self.check()?;
        self.inner.set_restore_mark()
    }

    fn clear_restore_mark(&mut self) -> Result<(), String> {
        self.check()?;
        self.inner.clear_restore_mark()
    }
}
