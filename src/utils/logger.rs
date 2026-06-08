use chrono::Local;

pub fn log(level: &str, module: &str, message: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    eprintln!("[{}] [{}/{}] {}", timestamp, level, module, message);
}

pub fn info(message: &str) { log("INFO", "app", message); }
pub fn warn(message: &str) { log("WARN", "app", message); }
pub fn error(message: &str) { log("ERROR", "app", message); }

pub mod module {
    pub fn info(m: &str, msg: &str) { super::log("INFO", m, msg); }
    pub fn warn(m: &str, msg: &str) { super::log("WARN", m, msg); }
    pub fn error(m: &str, msg: &str) { super::log("ERROR", m, msg); }
}
