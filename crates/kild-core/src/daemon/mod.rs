pub mod client;

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
