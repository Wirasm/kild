pub mod autostart;
pub mod client;
pub mod errors;

pub use autostart::ensure_daemon_running;
pub use errors::DaemonAutoStartError;

use std::path::PathBuf;

/// Default socket path for the daemon.
pub fn socket_path() -> PathBuf {
    let home = dirs::home_dir().expect("HOME not set");
    home.join(".kild").join("daemon.sock")
}

/// PID file path for the daemon process.
pub fn pid_file_path() -> PathBuf {
    let home = dirs::home_dir().expect("HOME not set");
    home.join(".kild").join("daemon.pid")
}
