//! What the home knows of the backup in progress (R1, R4): one job at a time, the Saver held
//! while an export reads the files, and the restore waiting for the restart. One state per home,
//! shared with the ticket of the job through an `Arc`, as `Updates`' state and its guard.

use std::sync::{Arc, Mutex};

use crate::devices::lock;
use crate::failure::{Failure, Outcome};

/// Where the backup is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// An export derives its key and waits for the Saver's write to end: the Saver still writes.
    Preparing,
    /// An export reads the files: the Saver starts no write.
    Exporting,
    /// A restore reads its body.
    Restoring,
    /// A restore is committed and waits for the restart (R6): both routes answer 409.
    Pending,
}

#[derive(Debug)]
struct Inner {
    phase: Phase,
    /// The Saver is writing a file (all its chunks and the rename).
    writing: bool,
}

/// The backup state of a home.
#[derive(Debug)]
pub struct BackupState {
    inner: Mutex<Inner>,
}

impl Default for BackupState {
    fn default() -> Self {
        BackupState {
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                writing: false,
            }),
        }
    }
}

impl BackupState {
    pub fn phase(&self) -> Phase {
        lock(&self.inner).phase
    }

    /// True when a restore is committed and waits for the restart.
    pub fn is_pending(&self) -> bool {
        self.phase() == Phase::Pending
    }

    /// Starts a job (R1): 409 `A restore waits for the restart` once one is committed, 409
    /// `Backup already in progress` while another runs. The ticket puts the state back to idle
    /// when it is dropped, unless its restore was committed.
    pub fn begin(self: &Arc<Self>, restore: bool) -> Outcome<BackupTicket> {
        let mut inner = lock(&self.inner);
        match inner.phase {
            Phase::Idle => {}
            Phase::Pending => return Err(Failure::new(409, "A restore waits for the restart")),
            _ => return Err(Failure::new(409, "Backup already in progress")),
        }
        inner.phase = if restore {
            Phase::Restoring
        } else {
            Phase::Preparing
        };
        Ok(BackupTicket {
            state: Arc::clone(self),
            committed: false,
        })
    }

    /// The Saver asks to write a file: `None` while an export reads the files (the file stays
    /// due), else the mark that says a write is on, until it is dropped. The check and the mark
    /// are one critical section, so an export cannot start between them (R4).
    pub fn try_write(self: &Arc<Self>) -> Option<WriteMark> {
        let mut inner = lock(&self.inner);
        if inner.phase == Phase::Exporting {
            return None;
        }
        inner.writing = true;
        Some(WriteMark {
            state: Arc::clone(self),
        })
    }

    /// The Saver writes whatever an export does (a stop): the mark only.
    pub fn force_write(self: &Arc<Self>) -> WriteMark {
        lock(&self.inner).writing = true;
        WriteMark {
            state: Arc::clone(self),
        }
    }
}

/// The Saver's `writing` mark: set under the lock of the state for the whole of a file's write,
/// cleared when dropped.
#[derive(Debug)]
pub struct WriteMark {
    state: Arc<BackupState>,
}

impl Drop for WriteMark {
    fn drop(&mut self) {
        lock(&self.state.inner).writing = false;
    }
}

/// The right to run one export or restore. Dropping it ends the job.
#[derive(Debug)]
pub struct BackupTicket {
    state: Arc<BackupState>,
    committed: bool,
}

impl BackupTicket {
    /// The export's switch to `exporting`: done only when the Saver is not writing, the check and
    /// the switch in one critical section (R4). False: the Saver is writing, ask again.
    pub fn hold_for_export(&self) -> bool {
        let mut inner = lock(&self.state.inner);
        if inner.writing {
            return false;
        }
        inner.phase = Phase::Exporting;
        true
    }

    /// The restore is committed: the state is `pending` and stays so (R6) until the restart.
    pub fn set_pending(&mut self) {
        self.committed = true;
        lock(&self.state.inner).phase = Phase::Pending;
    }
}

impl Drop for BackupTicket {
    fn drop(&mut self) {
        let mut inner = lock(&self.state.inner);
        if !self.committed {
            inner.phase = Phase::Idle;
        }
    }
}
