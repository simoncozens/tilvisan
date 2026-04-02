use std::io::{self, Write};
use std::sync::Once;

use log::{Level, LevelFilter, Log, Metadata, Record};

static STDERR_LOGGER: StderrLogger = StderrLogger;
static LOGGER_INIT: Once = Once::new();

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        match log::max_level() {
            LevelFilter::Off => false,
            LevelFilter::Error => metadata.level() <= Level::Error,
            LevelFilter::Warn => metadata.level() <= Level::Warn,
            LevelFilter::Info => metadata.level() <= Level::Info,
            LevelFilter::Debug => metadata.level() <= Level::Debug,
            LevelFilter::Trace => true,
        }
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "{}", record.args());
    }

    fn flush(&self) {
        let _ = io::stderr().flush();
    }
}

pub(crate) fn set_debug_logging(enabled: bool) {
    LOGGER_INIT.call_once(|| {
        let _ = log::set_logger(&STDERR_LOGGER);
    });

    log::set_max_level(if enabled {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    });
}
