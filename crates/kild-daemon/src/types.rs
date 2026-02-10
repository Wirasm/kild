use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Daemon-specific configuration.
///
/// Read from the `[daemon]` section of `~/.kild/config.toml`.
/// The daemon reads this itself; kild-core does not carry it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Path to the Unix domain socket.
    /// Default: `~/.kild/daemon.sock`
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,

    /// Path to the PID file.
    /// Default: `~/.kild/daemon.pid`
    #[serde(default = "default_pid_path")]
    pub pid_path: PathBuf,

    /// Per-session scrollback ring buffer size in bytes.
    /// Default: 65536 (64 KB)
    #[serde(default = "default_scrollback_buffer_size")]
    pub scrollback_buffer_size: usize,

    /// PTY output batching interval in milliseconds.
    /// Default: 4
    #[serde(default = "default_pty_output_batch_ms")]
    pub pty_output_batch_ms: u64,

    /// Per-client output buffer size before dropping oldest bytes.
    /// Default: 262144 (256 KB)
    #[serde(default = "default_client_buffer_size")]
    pub client_buffer_size: usize,

    /// Time in seconds to wait for agents to exit during shutdown.
    /// Default: 5
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
}

impl DaemonConfig {
    /// Validate configuration values.
    ///
    /// Called after loading config to catch misconfiguration early.
    pub fn validate(&self) -> Result<(), crate::errors::DaemonError> {
        if self.scrollback_buffer_size == 0 {
            return Err(crate::errors::DaemonError::ConfigInvalid(
                "scrollback_buffer_size must be > 0".to_string(),
            ));
        }
        if self.client_buffer_size == 0 {
            return Err(crate::errors::DaemonError::ConfigInvalid(
                "client_buffer_size must be > 0".to_string(),
            ));
        }
        if self.shutdown_timeout_secs == 0 {
            return Err(crate::errors::DaemonError::ConfigInvalid(
                "shutdown_timeout_secs must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            pid_path: default_pid_path(),
            scrollback_buffer_size: default_scrollback_buffer_size(),
            pty_output_batch_ms: default_pty_output_batch_ms(),
            client_buffer_size: default_client_buffer_size(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
        }
    }
}

fn default_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".kild")
        .join("daemon.sock")
}

fn default_pid_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".kild")
        .join("daemon.pid")
}

fn default_scrollback_buffer_size() -> usize {
    65536
}

fn default_pty_output_batch_ms() -> u64 {
    4
}

fn default_client_buffer_size() -> usize {
    262144
}

fn default_shutdown_timeout_secs() -> u64 {
    5
}

/// Wrapper for deserializing the `[daemon]` section from a KILD config file.
///
/// The daemon reads `~/.kild/config.toml` itself to extract its own configuration.
/// This struct mirrors just enough of the file structure to extract the `[daemon]` section.
#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    daemon: DaemonConfig,
}

/// Load daemon configuration from `~/.kild/config.toml`.
///
/// Reads the `[daemon]` section from the user's config file. Falls back to
/// defaults if the file doesn't exist or the section is missing.
pub fn load_daemon_config() -> Result<DaemonConfig, crate::errors::DaemonError> {
    let config_path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".kild")
        .join("config.toml");

    let config = match std::fs::read_to_string(&config_path) {
        Ok(contents) => match toml::from_str::<ConfigFile>(&contents) {
            Ok(file) => file.daemon,
            Err(e) => {
                tracing::warn!(
                    event = "daemon.config.parse_failed",
                    path = %config_path.display(),
                    error = %e,
                );
                DaemonConfig::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DaemonConfig::default(),
        Err(e) => {
            tracing::warn!(
                event = "daemon.config.read_failed",
                path = %config_path.display(),
                error = %e,
            );
            DaemonConfig::default()
        }
    };
    config.validate()?;
    Ok(config)
}

/// Runtime status of the daemon process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Daemon process PID.
    pub pid: u32,
    /// Seconds since daemon started.
    pub uptime_secs: u64,
    /// Number of managed sessions.
    pub session_count: usize,
    /// Number of sessions with active PTYs.
    pub active_connections: usize,
}

/// Summary of a daemon session as returned via IPC.
///
/// This is a PTY-centric wire type for the protocol, not the internal
/// `DaemonSession`. The daemon knows about PTYs and processes, not about
/// git worktrees or agents — those concepts live in kild-core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub working_directory: String,
    pub command: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_pid: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert!(config.socket_path.ends_with("daemon.sock"));
        assert_eq!(config.scrollback_buffer_size, 65536);
        assert_eq!(config.pty_output_batch_ms, 4);
        assert_eq!(config.client_buffer_size, 262144);
        assert_eq!(config.shutdown_timeout_secs, 5);
    }

    #[test]
    fn test_daemon_config_serde_roundtrip() {
        let config = DaemonConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: DaemonConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.scrollback_buffer_size, config.scrollback_buffer_size);
        assert_eq!(parsed.pty_output_batch_ms, config.pty_output_batch_ms);
        assert_eq!(parsed.client_buffer_size, config.client_buffer_size);
        assert_eq!(parsed.shutdown_timeout_secs, config.shutdown_timeout_secs);
    }

    #[test]
    fn test_daemon_status_serde_roundtrip() {
        let status = DaemonStatus {
            pid: 12345,
            uptime_secs: 3600,
            session_count: 3,
            active_connections: 2,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: DaemonStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pid, 12345);
        assert_eq!(parsed.uptime_secs, 3600);
        assert_eq!(parsed.session_count, 3);
        assert_eq!(parsed.active_connections, 2);
    }

    #[test]
    fn test_session_info_serde() {
        let info = SessionInfo {
            id: "myapp_feature-auth".to_string(),
            working_directory: "/tmp/worktrees/feature-auth".to_string(),
            command: "claude".to_string(),
            status: "running".to_string(),
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: Some(2),
            pty_pid: Some(12345),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, info.id);
        assert_eq!(parsed.command, "claude");
        assert_eq!(parsed.client_count, Some(2));
    }

    #[test]
    fn test_load_daemon_config_from_toml() {
        let toml = r#"
[daemon]
scrollback_buffer_size = 1024
shutdown_timeout_secs = 10
"#;
        let file: ConfigFile = toml::from_str(toml).unwrap();
        assert_eq!(file.daemon.scrollback_buffer_size, 1024);
        assert_eq!(file.daemon.shutdown_timeout_secs, 10);
        // Defaults for unset fields
        assert_eq!(file.daemon.pty_output_batch_ms, 4);
    }

    #[test]
    fn test_load_daemon_config_missing_section() {
        let toml = r#"
[agent]
default = "claude"
"#;
        let file: ConfigFile = toml::from_str(toml).unwrap();
        // Should get all defaults when [daemon] section is missing
        assert_eq!(file.daemon.scrollback_buffer_size, 65536);
        assert_eq!(file.daemon.shutdown_timeout_secs, 5);
    }

    #[test]
    fn test_validate_defaults_ok() {
        let config = DaemonConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_scrollback_fails() {
        let mut config = DaemonConfig::default();
        config.scrollback_buffer_size = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("scrollback_buffer_size"));
    }

    #[test]
    fn test_validate_zero_client_buffer_fails() {
        let mut config = DaemonConfig::default();
        config.client_buffer_size = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("client_buffer_size"));
    }

    #[test]
    fn test_validate_zero_shutdown_timeout_fails() {
        let mut config = DaemonConfig::default();
        config.shutdown_timeout_secs = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("shutdown_timeout_secs"));
    }

    #[test]
    fn test_session_info_optional_fields_omitted() {
        let info = SessionInfo {
            id: "test".to_string(),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: "stopped".to_string(),
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("client_count"));
        assert!(!json.contains("pty_pid"));
    }
}
