use serde::{Deserialize, Serialize};

/// Supported forge (code hosting platform) types.
///
/// Each variant represents a known code forge that can host
/// git repositories and provide PR/MR functionality.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeType {
    GitHub,
}

impl ForgeType {
    /// Get the canonical string name for this forge type.
    pub fn as_str(&self) -> &'static str {
        match self {
            ForgeType::GitHub => "github",
        }
    }
}

impl std::fmt::Display for ForgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ForgeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "github" => Ok(ForgeType::GitHub),
            _ => Err(format!("Unknown forge '{}'. Supported: github", s)),
        }
    }
}

/// Generate a newtype wrapper around `String` with standard trait impls.
///
/// Each generated type gets: `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`,
/// `Serialize`/`Deserialize` (transparent), `Display`, `Deref<Target=str>`,
/// `AsRef<str>`, `Borrow<str>`, `From<String>`, `From<&str>`.
macro_rules! newtype_string {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

newtype_string! {
    /// Unique identifier for a kild session (e.g., `"abc123def/feature-auth"`).
    SessionId
}

newtype_string! {
    /// User-facing branch name for a kild (e.g., `"feature-auth"`).
    ///
    /// This is the name the user provides, NOT the git branch ref (`"kild/feature-auth"`).
    BranchName
}

newtype_string! {
    /// Project identifier derived from the repository path hash (e.g., `"a1b2c3d4e5f6"`).
    ProjectId
}

/// PTY session status as reported by the daemon.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Creating,
    Running,
    Stopped,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Creating => write!(f, "creating"),
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Stopped => write!(f, "stopped"),
        }
    }
}

/// Summary of a daemon session as returned via IPC.
///
/// This is a PTY-centric wire type for the protocol, not the internal
/// `DaemonSession`. The daemon knows about PTYs and processes, not about
/// git worktrees or agents — those concepts live in kild-core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonSessionStatus {
    pub id: SessionId,
    pub working_directory: String,
    pub command: String,
    pub status: SessionStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// Agent-reported activity status, written via `kild agent-status` command.
///
/// This is distinct from `ProcessStatus` (running/stopped) and `HealthStatus`
/// (inferred from metrics). `AgentStatus` is explicitly reported by the agent
/// via hooks, giving real-time insight into what the agent is doing.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Working,
    Idle,
    Waiting,
    Done,
    Error,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Working => write!(f, "working"),
            Self::Idle => write!(f, "idle"),
            Self::Waiting => write!(f, "waiting"),
            Self::Done => write!(f, "done"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "working" => Ok(Self::Working),
            "idle" => Ok(Self::Idle),
            "waiting" => Ok(Self::Waiting),
            "done" => Ok(Self::Done),
            "error" => Ok(Self::Error),
            other => Err(format!(
                "Invalid agent status: '{}'. Valid: working, idle, waiting, done, error",
                other
            )),
        }
    }
}

/// How the agent process should be hosted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    #[serde(alias = "Terminal")]
    /// Launch in an external terminal window (Ghostty, iTerm, etc.)
    Terminal,
    #[serde(alias = "Daemon")]
    /// Launch in a daemon-owned PTY
    Daemon,
}

/// What to launch when opening a kild terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OpenMode {
    /// Launch the session's default agent (from config).
    DefaultAgent,
    /// Launch a specific agent (overrides session config).
    Agent(String),
    /// Open a bare terminal with `$SHELL` instead of an agent.
    BareShell,
}

/// What agent to launch when creating a kild.
///
/// Mirrors [`OpenMode`] for the create path. Determines whether the new kild
/// gets an AI agent or a bare terminal shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentMode {
    /// Use default agent from config.
    DefaultAgent,
    /// Use a specific agent (overrides config default).
    Agent(String),
    /// Open a bare terminal with `$SHELL` instead of an agent.
    BareShell,
}

impl From<AgentMode> for OpenMode {
    fn from(mode: AgentMode) -> Self {
        match mode {
            AgentMode::DefaultAgent => OpenMode::DefaultAgent,
            AgentMode::Agent(name) => OpenMode::Agent(name),
            AgentMode::BareShell => OpenMode::BareShell,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_session_status_serde() {
        let info = DaemonSessionStatus {
            id: SessionId::new("myapp_feature-auth"),
            working_directory: "/tmp/worktrees/feature-auth".to_string(),
            command: "claude".to_string(),
            status: SessionStatus::Running,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: Some(2),
            pty_pid: Some(12345),
            exit_code: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""status":"running""#));
        let parsed: DaemonSessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, info.id);
        assert_eq!(parsed.command, "claude");
        assert_eq!(parsed.status, SessionStatus::Running);
        assert_eq!(parsed.client_count, Some(2));
    }

    #[test]
    fn test_daemon_session_status_optional_fields_omitted() {
        let info = DaemonSessionStatus {
            id: SessionId::new("test"),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
            exit_code: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("client_count"));
        assert!(!json.contains("pty_pid"));
        assert!(!json.contains("exit_code"));
    }

    #[test]
    fn test_daemon_session_status_with_exit_code() {
        let info = DaemonSessionStatus {
            id: SessionId::new("test"),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
            exit_code: Some(1),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"exit_code\":1"));
    }

    #[test]
    fn test_daemon_session_status_exit_code_roundtrip() {
        let info = DaemonSessionStatus {
            id: SessionId::new("test"),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
            exit_code: Some(127),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: DaemonSessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.exit_code, Some(127));
    }

    #[test]
    fn test_session_status_display() {
        assert_eq!(SessionStatus::Creating.to_string(), "creating");
        assert_eq!(SessionStatus::Running.to_string(), "running");
        assert_eq!(SessionStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_session_status_roundtrip() {
        for status in [
            SessionStatus::Creating,
            SessionStatus::Running,
            SessionStatus::Stopped,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    // --- Newtype tests ---

    macro_rules! test_newtype {
        ($name:ident, $ty:ty) => {
            mod $name {
                use super::super::*;
                use std::collections::{HashMap, HashSet};

                #[test]
                fn serde_transparent_roundtrip() {
                    let val = <$ty>::new("test-value");
                    let json = serde_json::to_string(&val).unwrap();
                    assert_eq!(
                        json, r#""test-value""#,
                        "transparent serde should produce bare string"
                    );
                    let parsed: $ty = serde_json::from_str(&json).unwrap();
                    assert_eq!(parsed, val);
                }

                #[test]
                fn display() {
                    let val = <$ty>::new("hello");
                    assert_eq!(val.to_string(), "hello");
                }

                #[test]
                fn deref_to_str() {
                    let val = <$ty>::new("abc");
                    let s: &str = &val;
                    assert_eq!(s, "abc");
                    assert_eq!(val.len(), 3);
                }

                #[test]
                fn from_string() {
                    let val: $ty = String::from("owned").into();
                    assert_eq!(&*val, "owned");
                }

                #[test]
                fn from_str_ref() {
                    let val: $ty = "borrowed".into();
                    assert_eq!(&*val, "borrowed");
                }

                #[test]
                fn hash_set() {
                    let mut set = HashSet::new();
                    set.insert(<$ty>::new("a"));
                    set.insert(<$ty>::new("b"));
                    set.insert(<$ty>::new("a"));
                    assert_eq!(set.len(), 2);
                }

                #[test]
                fn borrow_str_hashmap_lookup() {
                    let mut map = HashMap::new();
                    map.insert(<$ty>::new("key"), 42);
                    assert_eq!(map.get("key"), Some(&42));
                }

                #[test]
                fn into_inner() {
                    let val = <$ty>::new("inner");
                    let s: String = val.into_inner();
                    assert_eq!(s, "inner");
                }

                #[test]
                fn as_ref_str() {
                    let val = <$ty>::new("ref-test");
                    let s: &str = val.as_ref();
                    assert_eq!(s, "ref-test");
                }

                #[test]
                fn empty_string() {
                    let val = <$ty>::new("");
                    assert_eq!(&*val, "");
                    assert_eq!(val.to_string(), "");
                }
            }
        };
    }

    test_newtype!(session_id, SessionId);
    test_newtype!(branch_name, BranchName);
    test_newtype!(project_id, ProjectId);

    #[test]
    fn test_session_status_wire_format() {
        assert_eq!(
            serde_json::to_string(&SessionStatus::Running).unwrap(),
            r#""running""#
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Stopped).unwrap(),
            r#""stopped""#
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Creating).unwrap(),
            r#""creating""#
        );
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Working.to_string(), "working");
        assert_eq!(AgentStatus::Idle.to_string(), "idle");
        assert_eq!(AgentStatus::Waiting.to_string(), "waiting");
        assert_eq!(AgentStatus::Done.to_string(), "done");
        assert_eq!(AgentStatus::Error.to_string(), "error");
    }

    #[test]
    fn test_agent_status_from_str() {
        assert_eq!(
            "working".parse::<AgentStatus>().unwrap(),
            AgentStatus::Working
        );
        assert_eq!("idle".parse::<AgentStatus>().unwrap(), AgentStatus::Idle);
        assert_eq!(
            "waiting".parse::<AgentStatus>().unwrap(),
            AgentStatus::Waiting
        );
        assert_eq!("done".parse::<AgentStatus>().unwrap(), AgentStatus::Done);
        assert_eq!("error".parse::<AgentStatus>().unwrap(), AgentStatus::Error);
        assert!("invalid".parse::<AgentStatus>().is_err());
    }

    #[test]
    fn test_agent_status_serde_roundtrip() {
        for status in [
            AgentStatus::Working,
            AgentStatus::Idle,
            AgentStatus::Waiting,
            AgentStatus::Done,
            AgentStatus::Error,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: AgentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_runtime_mode_serde_roundtrip() {
        for mode in [RuntimeMode::Terminal, RuntimeMode::Daemon] {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: RuntimeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn test_runtime_mode_deserializes_old_pascal_case() {
        assert_eq!(
            serde_json::from_str::<RuntimeMode>(r#""Terminal""#).unwrap(),
            RuntimeMode::Terminal
        );
        assert_eq!(
            serde_json::from_str::<RuntimeMode>(r#""Daemon""#).unwrap(),
            RuntimeMode::Daemon
        );
    }

    #[test]
    fn test_agent_mode_serde_roundtrip() {
        let modes = vec![
            AgentMode::DefaultAgent,
            AgentMode::Agent("claude".to_string()),
            AgentMode::BareShell,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let roundtripped: AgentMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, roundtripped);
        }
    }

    #[test]
    fn test_open_mode_serde_roundtrip() {
        let modes = vec![
            OpenMode::DefaultAgent,
            OpenMode::Agent("claude".to_string()),
            OpenMode::BareShell,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let roundtripped: OpenMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, roundtripped);
        }
    }

    #[test]
    fn test_agent_mode_into_open_mode_default() {
        let open: OpenMode = AgentMode::DefaultAgent.into();
        assert_eq!(open, OpenMode::DefaultAgent);
    }

    #[test]
    fn test_agent_mode_into_open_mode_agent() {
        let open: OpenMode = AgentMode::Agent("claude".to_string()).into();
        assert_eq!(open, OpenMode::Agent("claude".to_string()));
    }

    #[test]
    fn test_agent_mode_into_open_mode_bare_shell() {
        let open: OpenMode = AgentMode::BareShell.into();
        assert_eq!(open, OpenMode::BareShell);
    }

    #[test]
    fn test_runtime_mode_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&RuntimeMode::Terminal).unwrap(),
            r#""terminal""#
        );
        assert_eq!(
            serde_json::to_string(&RuntimeMode::Daemon).unwrap(),
            r#""daemon""#
        );
    }
}
