pub mod autostart;
pub mod client;
pub mod errors;
pub mod tofu;

pub use autostart::ensure_daemon_running;
pub use errors::DaemonAutoStartError;

use std::cell::RefCell;
use std::path::PathBuf;

use kild_paths::KildPaths;
use tracing::{debug, warn};

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

/// Bin file path for daemon binary staleness detection.
pub fn bin_file_path() -> PathBuf {
    KildPaths::resolve()
        .unwrap_or_else(|e| {
            warn!(
                event = "core.daemon.bin_path_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .daemon_bin_file()
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

/// Check if the running daemon binary is stale (binary updated since daemon started).
///
/// Reads the stored binary path + mtime from `~/.kild/daemon.bin` (written by the
/// daemon at startup) and compares it against the current `kild-daemon` binary on disk.
///
/// Returns `true` if the binary has been updated since the daemon started.
/// Returns `false` if the bin file is missing, unreadable, or the binary matches.
pub fn is_daemon_stale() -> bool {
    let bin_file = bin_file_path();
    let (stored_path, stored_mtime) = match read_bin_file(&bin_file) {
        Some(v) => v,
        None => return false,
    };

    let expected = match find_sibling_binary("kild-daemon") {
        Ok(p) => p,
        Err(_) => return false,
    };

    let expected_canonical = expected.canonicalize().unwrap_or(expected);

    // Path mismatch: binary moved to a different location
    if stored_path != expected_canonical {
        debug!(
            event = "core.daemon.stale_path_mismatch",
            stored = %stored_path.display(),
            expected = %expected_canonical.display(),
        );
        return true;
    }

    // Mtime mismatch: binary updated in-place (e.g., cargo install)
    let current_mtime = std::fs::metadata(&expected_canonical)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })
        .unwrap_or(0);

    if current_mtime != stored_mtime {
        debug!(
            event = "core.daemon.stale_mtime_mismatch",
            stored_mtime = stored_mtime,
            current_mtime = current_mtime,
            path = %stored_path.display(),
        );
        return true;
    }

    false
}

/// Read the binary path and mtime from the bin file.
fn read_bin_file(path: &std::path::Path) -> Option<(PathBuf, u64)> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return None,
    };
    let mut lines = content.lines();
    let bin_path = lines.next()?;
    let mtime_str = lines.next()?;
    let mtime = mtime_str.parse::<u64>().ok()?;
    Some((PathBuf::from(bin_path), mtime))
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
