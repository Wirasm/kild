use kild_paths::KildPaths;
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
    /// Default: 262144 (256 KB)
    #[serde(default = "default_scrollback_buffer_size")]
    pub scrollback_buffer_size: usize,

    /// PTY output batching interval in milliseconds.
    /// Default: 4
    #[serde(default = "default_pty_output_batch_ms")]
    pub pty_output_batch_ms: u64,

    /// Per-client output buffer size before dropping oldest bytes.
    /// Default: 1048576 (1 MB)
    #[serde(default = "default_client_buffer_size")]
    pub client_buffer_size: usize,

    /// Time in seconds to wait for agents to exit during shutdown.
    /// Default: 5
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,

    /// TCP listener address. None = Unix socket only.
    /// Example: "0.0.0.0:7432"
    #[serde(default)]
    pub bind_tcp: Option<std::net::SocketAddr>,

    /// Path to TLS certificate PEM file.
    /// Auto-generated at ~/.kild/certs/daemon.crt if None and bind_tcp is set.
    #[serde(default)]
    pub tls_cert_path: Option<PathBuf>,

    /// Path to TLS private key PEM file.
    /// Auto-generated at ~/.kild/certs/daemon.key if None and bind_tcp is set.
    #[serde(default)]
    pub tls_key_path: Option<PathBuf>,
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
        if self.client_buffer_size < 16_384 {
            return Err(crate::errors::DaemonError::ConfigInvalid(
                "client_buffer_size must be >= 16384 (16 KB)".to_string(),
            ));
        }
        if self.client_buffer_size > 104_857_600 {
            return Err(crate::errors::DaemonError::ConfigInvalid(
                "client_buffer_size must be <= 104857600 (100 MB)".to_string(),
            ));
        }
        if self.shutdown_timeout_secs == 0 {
            return Err(crate::errors::DaemonError::ConfigInvalid(
                "shutdown_timeout_secs must be > 0".to_string(),
            ));
        }
        // Validate TLS cert/key paths: must be specified together or not at all.
        match (&self.tls_cert_path, &self.tls_key_path) {
            (Some(_), None) => {
                return Err(crate::errors::DaemonError::ConfigInvalid(
                    "tls_cert_path is set but tls_key_path is missing".to_string(),
                ));
            }
            (None, Some(_)) => {
                return Err(crate::errors::DaemonError::ConfigInvalid(
                    "tls_key_path is set but tls_cert_path is missing".to_string(),
                ));
            }
            _ => {}
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
            bind_tcp: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

fn default_socket_path() -> PathBuf {
    KildPaths::resolve()
        .unwrap_or_else(|e| {
            tracing::warn!(
                event = "daemon.config.socket_path_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .daemon_socket()
}

fn default_pid_path() -> PathBuf {
    KildPaths::resolve()
        .unwrap_or_else(|e| {
            tracing::warn!(
                event = "daemon.config.pid_path_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .daemon_pid_file()
}

fn default_scrollback_buffer_size() -> usize {
    262_144
}

fn default_pty_output_batch_ms() -> u64 {
    4
}

fn default_client_buffer_size() -> usize {
    1_048_576
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
    let config_path = KildPaths::resolve()
        .unwrap_or_else(|e| {
            tracing::warn!(
                event = "daemon.config.home_dir_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .user_config();

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

pub use kild_protocol::{SessionInfo, SessionStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert!(config.socket_path.ends_with("daemon.sock"));
        assert_eq!(config.scrollback_buffer_size, 262_144);
        assert_eq!(config.pty_output_batch_ms, 4);
        assert_eq!(config.client_buffer_size, 1_048_576);
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
        assert_eq!(file.daemon.scrollback_buffer_size, 262_144);
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
    fn test_validate_small_client_buffer_fails() {
        let mut config = DaemonConfig::default();
        config.client_buffer_size = 1024;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("client_buffer_size"));
    }

    #[test]
    fn test_validate_huge_client_buffer_fails() {
        let mut config = DaemonConfig::default();
        config.client_buffer_size = 200_000_000;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("client_buffer_size"));
    }

    #[test]
    fn test_validate_min_client_buffer_ok() {
        let mut config = DaemonConfig::default();
        config.client_buffer_size = 16_384;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_shutdown_timeout_fails() {
        let mut config = DaemonConfig::default();
        config.shutdown_timeout_secs = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("shutdown_timeout_secs"));
    }

    #[test]
    fn test_daemon_config_tcp_fields_default_none() {
        let config = DaemonConfig::default();
        assert!(config.bind_tcp.is_none());
        assert!(config.tls_cert_path.is_none());
        assert!(config.tls_key_path.is_none());
    }

    #[test]
    fn test_daemon_config_bind_tcp_from_toml() {
        let toml = r#"
[daemon]
bind_tcp = "0.0.0.0:7432"
"#;
        let file: ConfigFile = toml::from_str(toml).unwrap();
        let addr = file.daemon.bind_tcp.unwrap();
        assert_eq!(addr.port(), 7432);
    }

    #[test]
    fn test_validate_cert_without_key_fails() {
        let mut config = DaemonConfig::default();
        config.tls_cert_path = Some(std::path::PathBuf::from("/tmp/daemon.crt"));
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("tls_cert_path"));
    }

    #[test]
    fn test_validate_key_without_cert_fails() {
        let mut config = DaemonConfig::default();
        config.tls_key_path = Some(std::path::PathBuf::from("/tmp/daemon.key"));
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("tls_key_path"));
    }

    #[test]
    fn test_validate_cert_and_key_together_ok() {
        let mut config = DaemonConfig::default();
        config.tls_cert_path = Some(std::path::PathBuf::from("/tmp/daemon.crt"));
        config.tls_key_path = Some(std::path::PathBuf::from("/tmp/daemon.key"));
        assert!(config.validate().is_ok());
    }
}
