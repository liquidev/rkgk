use alloc::format;

use log::{info, Log};

extern "C" {
    fn trace(message_len: u32, message: *const u8);
    fn debug(message_len: u32, message: *const u8);
    fn info(message_len: u32, message: *const u8);
    fn warn(message_len: u32, message: *const u8);
    fn error(message_len: u32, message: *const u8);
}

struct ConsoleLogger;

impl Log for ConsoleLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let s = record
            .module_path()
            .map(|module_path| format!("{module_path}: {}", record.args()))
            .unwrap_or_else(|| format!("{}", record.args()));
        unsafe {
            match record.level() {
                log::Level::Error => error(s.len() as u32, s.as_ptr()),
                log::Level::Warn => warn(s.len() as u32, s.as_ptr()),
                log::Level::Info => info(s.len() as u32, s.as_ptr()),
                log::Level::Debug => debug(s.len() as u32, s.as_ptr()),
                log::Level::Trace => trace(s.len() as u32, s.as_ptr()),
            }
        }
    }

    fn flush(&self) {}
}

#[no_mangle]
extern "C" fn haku_init_logging() {
    log::set_logger(&ConsoleLogger).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    info!("enabled logging");
}
