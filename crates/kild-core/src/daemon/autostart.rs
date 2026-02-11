use tracing::{error, info, warn};

use crate::config::KildConfig;
use crate::daemon::{client, socket_path};

use super::errors::DaemonAutoStartError;

/// Ensure the daemon is running, auto-starting it if configured.
///
/// 1. Pings the daemon — if alive, returns immediately.
/// 2. Checks `config.daemon_auto_start()` — if disabled, returns `Disabled` error.
/// 3. Spawns `kild-daemon` binary in background (stderr inherited).
/// 4. Polls socket + ping with 5s timeout, 100ms interval. Checks child process
///    exit status each iteration to detect early crashes.
pub fn ensure_daemon_running(config: &KildConfig) -> Result<(), DaemonAutoStartError> {
    match client::ping_daemon() {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(e) => {
            warn!(event = "core.daemon.ping_check_failed", error = %e);
        }
    }

    if !config.daemon_auto_start() {
        return Err(DaemonAutoStartError::Disabled);
    }

    info!(event = "core.daemon.auto_start_started");
    eprintln!("Starting daemon...");

    let daemon_binary = super::find_sibling_binary("kild-daemon")
        .map_err(|message| DaemonAutoStartError::BinaryNotFound { message })?;

    let mut child = std::process::Command::new(&daemon_binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| DaemonAutoStartError::SpawnFailed {
            message: e.to_string(),
        })?;

    let socket = socket_path();
    let timeout = std::time::Duration::from_secs(5);
    let start = std::time::Instant::now();

    loop {
        // Check if daemon process crashed before socket was ready
        match child.try_wait() {
            Ok(Some(status)) => {
                error!(event = "core.daemon.auto_start_failed", reason = "child_exited", status = %status);
                return Err(DaemonAutoStartError::SpawnFailed {
                    message: format!(
                        "Daemon process exited with {} before becoming ready.\n\
                         Check daemon logs: kild daemon start --foreground\n\
                         Daemon binary: {}",
                        status,
                        daemon_binary.display()
                    ),
                });
            }
            Ok(None) => {} // Still running
            Err(e) => {
                warn!(event = "core.daemon.child_status_check_failed", error = %e);
            }
        }

        if socket.exists() && client::ping_daemon().unwrap_or(false) {
            info!(event = "core.daemon.auto_start_completed");
            eprintln!("Daemon started.");
            return Ok(());
        }

        if start.elapsed() > timeout {
            let socket_exists = socket.exists();
            if socket_exists {
                error!(
                    event = "core.daemon.auto_start_failed",
                    reason = "timeout_no_ping",
                    socket_exists = true
                );
                return Err(DaemonAutoStartError::Timeout {
                    message: "Daemon socket exists but not responding to ping after 5s.\n\
                              Try: kild daemon stop && kild daemon start"
                        .to_string(),
                });
            } else {
                error!(
                    event = "core.daemon.auto_start_failed",
                    reason = "timeout_no_socket",
                    socket_exists = false
                );
                return Err(DaemonAutoStartError::Timeout {
                    message: format!(
                        "Daemon process spawned but socket not created after 5s.\n\
                         Check daemon logs: kild daemon start --foreground\n\
                         Daemon binary: {}",
                        daemon_binary.display()
                    ),
                });
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::errors::SessionError;

    #[test]
    fn test_auto_start_disabled_returns_error() {
        // Early return if daemon is already running — we can't test the Disabled
        // error path when daemon is active because ensure_daemon_running() exits
        // before checking config.
        if client::ping_daemon().unwrap_or(false) {
            return;
        }

        let mut value = serde_json::to_value(KildConfig::default()).unwrap();
        value["daemon"]["auto_start"] = serde_json::Value::Bool(false);
        let config: KildConfig = serde_json::from_value(value).unwrap();

        let result = ensure_daemon_running(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, DaemonAutoStartError::Disabled));

        let msg = err.to_string();
        assert!(
            msg.contains("kild daemon start"),
            "should mention manual start"
        );
        assert!(
            msg.contains("auto_start = true"),
            "should mention config option"
        );
        assert!(
            msg.contains("--no-daemon"),
            "should mention --no-daemon flag"
        );
    }

    #[test]
    fn test_auto_start_default_config_is_enabled() {
        assert!(KildConfig::default().daemon_auto_start());
    }

    #[test]
    fn test_auto_start_error_display_messages() {
        let disabled = DaemonAutoStartError::Disabled;
        assert!(disabled.to_string().contains("not running"));

        let spawn_failed = DaemonAutoStartError::SpawnFailed {
            message: "permission denied".to_string(),
        };
        assert!(spawn_failed.to_string().contains("permission denied"));

        let timeout = DaemonAutoStartError::Timeout {
            message: "socket not created".to_string(),
        };
        assert!(timeout.to_string().contains("socket not created"));

        let not_found = DaemonAutoStartError::BinaryNotFound {
            message: "no such file".to_string(),
        };
        assert!(not_found.to_string().contains("no such file"));
    }

    #[test]
    fn test_auto_start_succeeds_when_daemon_running() {
        // Only meaningful when daemon is actually running
        if !client::ping_daemon().unwrap_or(false) {
            return;
        }

        // Even with auto_start=false, should succeed because daemon is already running
        let mut value = serde_json::to_value(KildConfig::default()).unwrap();
        value["daemon"]["auto_start"] = serde_json::Value::Bool(false);
        let config: KildConfig = serde_json::from_value(value).unwrap();

        let result = ensure_daemon_running(&config);
        assert!(result.is_ok(), "Should succeed when daemon already running");
    }

    #[test]
    fn test_auto_start_error_converts_to_session_error() {
        let err = DaemonAutoStartError::Disabled;
        let session_err: SessionError = err.into();
        assert!(
            matches!(session_err, SessionError::DaemonAutoStartFailed { .. }),
            "Should convert to DaemonAutoStartFailed variant"
        );
    }
}
