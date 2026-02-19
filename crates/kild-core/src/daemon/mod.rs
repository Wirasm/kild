pub mod autostart;
pub mod client;
pub mod errors;
pub mod tofu;

pub use autostart::ensure_daemon_running;
pub use errors::DaemonAutoStartError;

use std::cell::RefCell;
use std::path::PathBuf;

use kild_paths::KildPaths;
use tracing::warn;

/// Default socket path for the daemon.
pub fn socket_path() -> PathBuf {
    KildPaths::resolve()
        .unwrap_or_else(|e| {
            warn!(
                event = "core.daemon.socket_path_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .daemon_socket()
}

/// PID file path for the daemon process.
pub fn pid_file_path() -> PathBuf {
    KildPaths::resolve()
        .unwrap_or_else(|e| {
            warn!(
                event = "core.daemon.pid_path_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .daemon_pid_file()
}

/// Find a sibling binary next to the currently running executable.
///
/// Looks for `binary_name` in the same directory as `std::env::current_exe()`.
/// Returns the full path if found, or a descriptive error if not.
pub fn find_sibling_binary(binary_name: &str) -> Result<PathBuf, String> {
    let our_binary =
        std::env::current_exe().map_err(|e| format!("could not determine binary path: {}", e))?;
    let bin_dir = our_binary
        .parent()
        .ok_or_else(|| format!("binary has no parent directory: {}", our_binary.display()))?;
    let sibling = bin_dir.join(binary_name);
    if !sibling.exists() {
        return Err(format!(
            "{} binary not found at {}. Run 'cargo build --all' to build it.",
            binary_name,
            sibling.display()
        ));
    }
    Ok(sibling)
}

// TODO: replace with explicit context parameter threaded through command handlers.
// Thread-local override lets `--remote` CLI flag take precedence over config
// without touching every command handler signature. Acceptable for the CLI
// (single-threaded, one invocation = one call path).
thread_local! {
    static REMOTE_OVERRIDE: RefCell<Option<(String, Option<String>)>> = const { RefCell::new(None) };
}

/// Set a process-wide remote daemon override from CLI flags.
///
/// When set, `get_connection()` in `client.rs` connects via TCP/TLS
/// regardless of the config file. Takes precedence over `remote_host` in config.
pub fn set_remote_override(host: &str, fingerprint: Option<&str>) {
    REMOTE_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = Some((host.to_string(), fingerprint.map(|s| s.to_string())));
    });
}

/// Read the current remote override, if any.
pub(crate) fn remote_override() -> Option<(String, Option<String>)> {
    REMOTE_OVERRIDE.with(|cell| cell.borrow().clone())
}
