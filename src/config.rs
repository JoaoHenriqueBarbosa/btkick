//! Tiny persistence for the default device MAC, stored as a single line in
//! ~/.config/btkick/default. No serde — it's one string.

use std::fs;
use std::path::PathBuf;

fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        });
    base.join("btkick")
}

fn default_file() -> PathBuf {
    config_dir().join("default")
}

pub fn read_default() -> Option<String> {
    let s = fs::read_to_string(default_file()).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn write_default(mac: &str) -> std::io::Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::write(default_file(), format!("{mac}\n"))
}

fn prev_sink_file() -> PathBuf {
    config_dir().join("prev_sink")
}

/// The audio sink that was default *before* we switched to the device, so we
/// can restore exactly it on disconnect instead of guessing.
pub fn read_prev_sink() -> Option<String> {
    let s = fs::read_to_string(prev_sink_file()).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn write_prev_sink(sink: &str) {
    let _ = fs::create_dir_all(config_dir());
    let _ = fs::write(prev_sink_file(), format!("{sink}\n"));
}

pub fn clear_prev_sink() {
    let _ = fs::remove_file(prev_sink_file());
}
