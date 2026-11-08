//! Minimal file logger: TopNote's own messages go to `topnote.log` in the config folder
//! (Settings → Diagnostics → Open), so failures like a fallback to Spotify can be explained.
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

struct FileLog(Mutex<File>);

const MAX_BYTES: u64 = 512 * 1024;

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info && m.target().starts_with("topnote")
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut f) = self.0.lock() {
            let _ = writeln!(f, "{secs} {:<5} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {
        if let Ok(mut f) = self.0.lock() {
            let _ = f.flush();
        }
    }
}

pub fn init(dir: &Path) {
    let path = dir.join("topnote.log");
    // start over once it gets big; it only needs to explain recent behaviour
    let big = std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    let file = if big { File::create(&path) } else { OpenOptions::new().create(true).append(true).open(&path) };
    if let Ok(f) = file {
        if log::set_logger(Box::leak(Box::new(FileLog(Mutex::new(f))))).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
            log::info!("TopNote {} started", env!("CARGO_PKG_VERSION"));
        }
    }
}
