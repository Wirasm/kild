#[cfg(target_os = "macos")]
mod macos;
mod types;

pub use types::NativeWindow;

#[cfg(target_os = "macos")]
pub use macos::{find_window, find_window_by_pid, focus_window, minimize_window};
