use log::{Level, LevelFilter, Metadata, Record};
use rtt_target::{rprint, rprintln};

pub(crate) struct RttLogger {
    level: Level,
}

const LONGEST_LEVEL_LEN: usize = /* TRACE */ 5;

impl log::Log for RttLogger {
    fn enabled(&self, meta: &Metadata) -> bool {
        meta.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            // Print log level
            rprint!("{: >width$} |", record.level(), width = LONGEST_LEVEL_LEN);
            // Print message
            rprintln!("{}", record.args());
        }
    }

    fn flush(&self) {}
}

pub fn init(level: Level) {
    static LOGGER: RttLogger = RttLogger { level };
    // SAFETY: there are no other loggers on system that could race against this
    unsafe {
        log::set_logger_racy(&LOGGER)
            .map(|()| {
                log::set_max_level_racy(
                    option_env!("LOG_LEVEL")
                        .map(|s| s.parse().unwrap())
                        .unwrap_or(LevelFilter::Info),
                )
            })
            .unwrap();
    };
}
