//! Lightweight opt-in file logging for diagnosing the mouse-hook behaviour.
//!
//! Logging is enabled only when the `EMR_LOG` environment variable is set:
//!   EMR_LOG=1       -> log next to the exe (<exe dir>\easy-move-resize.log)
//!   EMR_LOG=<path>  -> log to that file
//! When unset, every `log!` is an inexpensive no-op (one atomic load).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static SINK: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// Resolve the log path from `EMR_LOG`, or `None` if logging is disabled.
fn log_path() -> Option<PathBuf> {
    let val = std::env::var_os("EMR_LOG")?;
    let val = val.to_string_lossy().into_owned();
    if val.is_empty() {
        return None;
    }
    if val == "1" {
        let mut p = std::env::current_exe().ok()?;
        p.set_file_name("easy-move-resize.log");
        Some(p)
    } else {
        Some(PathBuf::from(val))
    }
}

/// Initialise logging once at startup. Safe (and cheap) to call before any `log!`.
pub fn init() {
    let sink = log_path().and_then(|p| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .ok()
            .map(Mutex::new)
    });
    let _ = SINK.set(sink);
    let _ = START.set(Instant::now());
}

/// Whether logging is currently active.
pub fn enabled() -> bool {
    matches!(SINK.get(), Some(Some(_)))
}

/// Write one timestamped line. No-op when logging is disabled.
pub fn write(args: std::fmt::Arguments) {
    let Some(Some(mutex)) = SINK.get() else {
        return;
    };
    let ms = START.get().map(|s| s.elapsed().as_millis()).unwrap_or(0);
    if let Ok(mut f) = mutex.lock() {
        let _ = writeln!(f, "[{ms:>9} ms] {args}");
        let _ = f.flush();
    }
}
