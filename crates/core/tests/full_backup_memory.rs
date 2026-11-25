//! Spec 035 R9: an archive of 1 MB is exported, then restored, through one buffer of 4 KB, the
//! hash, MAC and cipher states: the peak of the heap stays under 32 KB each way, whatever the
//! size. The files are made on the fly and the staged copies only counted: nothing holds a byte.
//! One test only in this file, so the counting allocator sees nothing else.

use std::cell::Cell;
use std::io::{self, Read, Write};
use std::rc::Rc;

use peak_alloc::PeakAlloc;
use solite_core::backup::{BackupFiles, Pacing, StagedFile};
use solite_core::home::Home;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

const FILE_BYTES: usize = 512 * 1024;
const PEAK_BOUND: usize = 32 * 1024;

/// A file of `FILE_BYTES` bytes, made as it is read.
struct Generated {
    position: usize,
}

impl Read for Generated {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = out.len().min(FILE_BYTES - self.position);
        for (i, slot) in out[..count].iter_mut().enumerate() {
            *slot = b'a' + u8::try_from((self.position + i) % 26).unwrap_or(0);
        }
        self.position += count;
        Ok(count)
    }
}

/// A staged copy that keeps nothing: it counts.
struct Counting {
    written: Rc<Cell<usize>>,
}

impl Write for Counting {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written.set(self.written.get() + bytes.len());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl StagedFile for Counting {
    fn finish(self: Box<Self>) -> Result<(), String> {
        Ok(())
    }
}

/// Two files of 512 KB, `devices.json` and `zones.json`; nothing else.
struct Files {
    written: Rc<Cell<usize>>,
}

impl BackupFiles for Files {
    fn has_wifi(&self) -> bool {
        false
    }
    fn restarts(&self) -> bool {
        true
    }
    fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        Ok(matches!(name, "devices.json" | "zones.json")
            .then(|| Box::new(Generated { position: 0 }) as Box<dyn Read>))
    }
    fn stage(&mut self, _: &str) -> Result<Box<dyn StagedFile>, String> {
        Ok(Box::new(Counting {
            written: Rc::clone(&self.written),
        }))
    }
    fn open_staged(&mut self, _: &str) -> Result<Option<Box<dyn Read>>, String> {
        Ok(None)
    }
    fn discard(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn commit(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn pending(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn promote(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn remove(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn set_aside(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn clear_pending(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn restore_mark(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn set_restore_mark(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn clear_restore_mark(&mut self) -> Result<(), String> {
        Ok(())
    }
}

struct NoPacing;

impl Pacing for NoPacing {
    fn now_ms(&mut self) -> u64 {
        0
    }
    fn pause(&mut self) {}
}

fn random(buffer: &mut [u8]) -> Result<(), String> {
    buffer.fill(0x5A);
    Ok(())
}

#[test]
fn an_archive_of_a_megabyte_is_exported_then_restored_under_32_kb_of_heap_each_way() {
    let written = Rc::new(Cell::new(0));
    // The homes and their tickets are made before the measure: they are not the backup's.
    let home = Home::new(0);
    let export_ticket = home.backup_begin(false).unwrap();
    let other = Home::new(0);
    let restore_ticket = other.backup_begin(true).unwrap();
    let password = "a%20long%20enough%20password";

    // Export: the archive is read into a stack buffer and counted.
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();
    let mut archive = export_ticket
        .export(
            Some(password),
            Box::new(Files {
                written: Rc::clone(&written),
            }),
            Box::new(NoPacing),
            &mut random,
        )
        .unwrap_or_else(|e| panic!("export refused: {e:?}"));
    let mut buffer = [0u8; 8192];
    let mut total = 0usize;
    loop {
        let count = archive.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        total += count;
    }
    drop(archive);
    let export_peak = ALLOCATOR.peak_usage().saturating_sub(base);
    assert!(total > 2 * FILE_BYTES, "{total} bytes");
    println!("backup: {total} bytes exported, peak heap {export_peak} bytes");
    assert!(export_peak < PEAK_BOUND, "export peak {export_peak} bytes");

    // Restore: the body is a live export (nothing holds it); the peak counts both ends.
    let feeder_ticket = home.backup_begin(false).unwrap();
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();
    let mut source = feeder_ticket
        .export(
            Some(password),
            Box::new(Files {
                written: Rc::clone(&written),
            }),
            Box::new(NoPacing),
            &mut random,
        )
        .unwrap_or_else(|e| panic!("export refused: {e:?}"));
    let response = restore_ticket.restore(
        false,
        None,
        Some(password),
        &mut source,
        &mut Files {
            written: Rc::clone(&written),
        },
        &mut NoPacing,
    );
    let restore_peak = ALLOCATOR.peak_usage().saturating_sub(base);
    assert_eq!(response.status, 200, "{:?}", response.body);
    assert_eq!(written.get(), 2 * FILE_BYTES, "both files staged");
    println!(
        "backup: {total} bytes restored, peak heap {restore_peak} bytes (export end included)"
    );
    assert!(
        restore_peak < PEAK_BOUND,
        "restore peak {restore_peak} bytes"
    );
}
