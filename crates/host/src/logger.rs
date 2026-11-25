//! A minimal logger on stderr: warnings and errors of the core and of the host, so that what the
//! radio reports (overflow, dropped callbacks...) is seen on the Mac. The `update:` lines of the
//! core (installed, confirmed...) are shown too, on the standard output like the host's own lines.

use log::{Level, LevelFilter, Log, Metadata, Record};

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if record.level() <= Level::Warn {
            eprintln!(
                "{}: {}",
                record.level().as_str().to_lowercase(),
                record.args()
            );
        } else if record.level() == Level::Info && record.args().to_string().starts_with("update:")
        {
            println!("{}", record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

/// Installs the logger (warnings and above, and the information lines that start with `update:`).
/// A second call does nothing.
pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}
