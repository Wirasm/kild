use serde::{Deserialize, Serialize};

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
pub struct SessionInfo {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info_serde() {
        let info = SessionInfo {
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
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, info.id);
        assert_eq!(parsed.command, "claude");
        assert_eq!(parsed.status, SessionStatus::Running);
        assert_eq!(parsed.client_count, Some(2));
    }

    #[test]
    fn test_session_info_optional_fields_omitted() {
        let info = SessionInfo {
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
    fn test_session_info_with_exit_code() {
        let info = SessionInfo {
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
    fn test_session_info_exit_code_roundtrip() {
        let info = SessionInfo {
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
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
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
}
