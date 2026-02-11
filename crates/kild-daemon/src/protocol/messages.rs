use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::SessionInfo;

/// Client -> Daemon request messages.
///
/// Each variant maps to a JSONL message with `"type"` as the tag field.
/// All requests carry an `id` field for response correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Create a new daemon session with a PTY.
    ///
    /// The daemon does NOT create git worktrees — that is the caller's
    /// responsibility. The daemon just spawns a process in a PTY.
    #[serde(rename = "create_session")]
    CreateSession {
        id: String,
        /// Unique session identifier (e.g. "myapp_feature-auth").
        session_id: String,
        /// Working directory for the PTY process.
        working_directory: String,
        /// Command to execute in the PTY.
        command: String,
        /// Arguments for the command.
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables to set for the PTY process.
        #[serde(default)]
        env_vars: HashMap<String, String>,
        /// Initial PTY rows.
        #[serde(default = "default_rows")]
        rows: u16,
        /// Initial PTY columns.
        #[serde(default = "default_cols")]
        cols: u16,
        /// When true, use `CommandBuilder::new_default_prog()` for a native login shell
        /// instead of `CommandBuilder::new(command)`. Used for bare shell sessions.
        #[serde(default)]
        use_login_shell: bool,
    },

    #[serde(rename = "attach")]
    Attach {
        id: String,
        session_id: String,
        rows: u16,
        cols: u16,
    },

    #[serde(rename = "detach")]
    Detach { id: String, session_id: String },

    #[serde(rename = "resize_pty")]
    ResizePty {
        id: String,
        session_id: String,
        rows: u16,
        cols: u16,
    },

    #[serde(rename = "write_stdin")]
    WriteStdin {
        id: String,
        session_id: String,
        /// Base64-encoded bytes to write to PTY stdin.
        data: String,
    },

    #[serde(rename = "stop_session")]
    StopSession { id: String, session_id: String },

    #[serde(rename = "destroy_session")]
    DestroySession {
        id: String,
        session_id: String,
        #[serde(default)]
        force: bool,
    },

    #[serde(rename = "list_sessions")]
    ListSessions {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        project_id: Option<String>,
    },

    #[serde(rename = "get_session")]
    GetSession { id: String, session_id: String },

    #[serde(rename = "read_scrollback")]
    ReadScrollback { id: String, session_id: String },

    #[serde(rename = "daemon_stop")]
    DaemonStop { id: String },

    #[serde(rename = "ping")]
    Ping { id: String },
}

/// Daemon -> Client response and streaming messages.
///
/// Each variant maps to a JSONL message with `"type"` as the tag field.
/// Response messages echo the request `id`. Streaming messages have no `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonMessage {
    #[serde(rename = "session_created")]
    SessionCreated { id: String, session: SessionInfo },

    /// Streaming PTY output. No `id` — pushed after attach.
    #[serde(rename = "pty_output")]
    PtyOutput {
        session_id: String,
        /// Base64-encoded raw PTY output bytes.
        data: String,
    },

    /// Notification that PTY output was dropped for a slow client.
    #[serde(rename = "pty_output_dropped")]
    PtyOutputDropped {
        session_id: String,
        bytes_dropped: usize,
    },

    /// Session state change notification. No `id`.
    #[serde(rename = "session_event")]
    SessionEvent {
        event: String,
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },

    #[serde(rename = "session_list")]
    SessionList {
        id: String,
        sessions: Vec<SessionInfo>,
    },

    #[serde(rename = "session_info")]
    SessionInfo { id: String, session: SessionInfo },

    #[serde(rename = "scrollback_contents")]
    ScrollbackContents {
        id: String,
        /// Base64-encoded raw scrollback bytes.
        data: String,
    },

    #[serde(rename = "error")]
    Error {
        id: String,
        code: String,
        message: String,
    },

    #[serde(rename = "ack")]
    Ack { id: String },
}

fn default_rows() -> u16 {
    24
}

fn default_cols() -> u16 {
    80
}

impl ClientMessage {
    /// Extract the request ID from any client message.
    pub fn id(&self) -> &str {
        match self {
            ClientMessage::CreateSession { id, .. }
            | ClientMessage::Attach { id, .. }
            | ClientMessage::Detach { id, .. }
            | ClientMessage::ResizePty { id, .. }
            | ClientMessage::WriteStdin { id, .. }
            | ClientMessage::StopSession { id, .. }
            | ClientMessage::DestroySession { id, .. }
            | ClientMessage::ListSessions { id, .. }
            | ClientMessage::GetSession { id, .. }
            | ClientMessage::ReadScrollback { id, .. }
            | ClientMessage::DaemonStop { id, .. }
            | ClientMessage::Ping { id, .. } => id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_create_session_roundtrip() {
        let msg = ClientMessage::CreateSession {
            id: "req-001".to_string(),
            session_id: "myapp_feature-auth".to_string(),
            working_directory: "/tmp/worktrees/feature-auth".to_string(),
            command: "claude".to_string(),
            args: vec!["--dangerously-skip-permissions".to_string()],
            env_vars: HashMap::from([(
                "KILD_SESSION".to_string(),
                "myapp_feature-auth".to_string(),
            )]),
            rows: 24,
            cols: 80,
            use_login_shell: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"create_session"#));
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), "req-001");
    }

    #[test]
    fn test_client_message_attach_roundtrip() {
        let msg = ClientMessage::Attach {
            id: "req-002".to_string(),
            session_id: "myapp_feature-auth".to_string(),
            rows: 24,
            cols: 80,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), "req-002");
    }

    #[test]
    fn test_client_message_write_stdin_roundtrip() {
        let msg = ClientMessage::WriteStdin {
            id: "req-005".to_string(),
            session_id: "myapp_feature-auth".to_string(),
            data: "bHMgLWxhCg==".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), "req-005");
    }

    #[test]
    fn test_client_message_daemon_stop_roundtrip() {
        let msg = ClientMessage::DaemonStop {
            id: "req-010".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"daemon_stop"#));
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), "req-010");
    }

    #[test]
    fn test_client_message_all_variants_roundtrip() {
        let messages: Vec<ClientMessage> = vec![
            ClientMessage::CreateSession {
                id: "1".to_string(),
                session_id: "s".to_string(),
                working_directory: "/tmp".to_string(),
                command: "bash".to_string(),
                args: vec![],
                use_login_shell: false,
                env_vars: HashMap::new(),
                rows: 24,
                cols: 80,
            },
            ClientMessage::Attach {
                id: "2".to_string(),
                session_id: "s".to_string(),
                rows: 24,
                cols: 80,
            },
            ClientMessage::Detach {
                id: "3".to_string(),
                session_id: "s".to_string(),
            },
            ClientMessage::ResizePty {
                id: "4".to_string(),
                session_id: "s".to_string(),
                rows: 40,
                cols: 120,
            },
            ClientMessage::WriteStdin {
                id: "5".to_string(),
                session_id: "s".to_string(),
                data: "dGVzdA==".to_string(),
            },
            ClientMessage::StopSession {
                id: "6".to_string(),
                session_id: "s".to_string(),
            },
            ClientMessage::DestroySession {
                id: "7".to_string(),
                session_id: "s".to_string(),
                force: true,
            },
            ClientMessage::ListSessions {
                id: "8".to_string(),
                project_id: None,
            },
            ClientMessage::GetSession {
                id: "9".to_string(),
                session_id: "s".to_string(),
            },
            ClientMessage::ReadScrollback {
                id: "9b".to_string(),
                session_id: "s".to_string(),
            },
            ClientMessage::DaemonStop {
                id: "10".to_string(),
            },
            ClientMessage::Ping {
                id: "11".to_string(),
            },
        ];

        for msg in messages {
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.id(), msg.id());
        }
    }

    #[test]
    fn test_daemon_message_session_created_roundtrip() {
        let msg = DaemonMessage::SessionCreated {
            id: "req-001".to_string(),
            session: SessionInfo {
                id: "myapp_feature-auth".to_string(),
                working_directory: "/tmp/worktrees/feature-auth".to_string(),
                command: "claude".to_string(),
                status: "running".to_string(),
                created_at: "2026-02-09T14:30:00Z".to_string(),
                client_count: None,
                pty_pid: None,
                exit_code: None,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"session_created"#));
        let parsed: DaemonMessage = serde_json::from_str(&json).unwrap();
        if let DaemonMessage::SessionCreated { id, session } = parsed {
            assert_eq!(id, "req-001");
            assert_eq!(session.command, "claude");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_daemon_message_pty_output_roundtrip() {
        let msg = DaemonMessage::PtyOutput {
            session_id: "myapp_feature-auth".to_string(),
            data: "dG90YWwgNDgK".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"pty_output"#));
        let parsed: DaemonMessage = serde_json::from_str(&json).unwrap();
        if let DaemonMessage::PtyOutput { session_id, data } = parsed {
            assert_eq!(session_id, "myapp_feature-auth");
            assert_eq!(data, "dG90YWwgNDgK");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_daemon_message_error_roundtrip() {
        let msg = DaemonMessage::Error {
            id: "req-001".to_string(),
            code: "session_not_found".to_string(),
            message: "No session found with id 'myapp_feature-auth'".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: DaemonMessage = serde_json::from_str(&json).unwrap();
        if let DaemonMessage::Error { id, code, message } = parsed {
            assert_eq!(id, "req-001");
            assert_eq!(code, "session_not_found");
            assert!(message.contains("myapp_feature-auth"));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_daemon_message_ack_roundtrip() {
        let msg = DaemonMessage::Ack {
            id: "req-003".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"ack"#));
        let parsed: DaemonMessage = serde_json::from_str(&json).unwrap();
        if let DaemonMessage::Ack { id } = parsed {
            assert_eq!(id, "req-003");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_daemon_message_session_event_roundtrip() {
        let msg = DaemonMessage::SessionEvent {
            event: "stopped".to_string(),
            session_id: "myapp_feature-auth".to_string(),
            details: Some(serde_json::json!({"exit_code": 0})),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: DaemonMessage = serde_json::from_str(&json).unwrap();
        if let DaemonMessage::SessionEvent {
            event,
            session_id,
            details,
        } = parsed
        {
            assert_eq!(event, "stopped");
            assert_eq!(session_id, "myapp_feature-auth");
            assert!(details.is_some());
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_daemon_message_pty_output_dropped_roundtrip() {
        let msg = DaemonMessage::PtyOutputDropped {
            session_id: "test".to_string(),
            bytes_dropped: 4096,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: DaemonMessage = serde_json::from_str(&json).unwrap();
        if let DaemonMessage::PtyOutputDropped {
            session_id,
            bytes_dropped,
        } = parsed
        {
            assert_eq!(session_id, "test");
            assert_eq!(bytes_dropped, 4096);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_client_message_create_session_defaults() {
        // Empty args and env_vars default to empty, rows/cols default to 24/80
        let json = r#"{"id":"1","type":"create_session","session_id":"s","working_directory":"/tmp","command":"bash"}"#;
        let parsed: ClientMessage = serde_json::from_str(json).unwrap();
        if let ClientMessage::CreateSession {
            args,
            env_vars,
            rows,
            cols,
            ..
        } = parsed
        {
            assert!(args.is_empty());
            assert!(env_vars.is_empty());
            assert_eq!(rows, 24);
            assert_eq!(cols, 80);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_client_message_id_extraction() {
        let msg = ClientMessage::ListSessions {
            id: "my-id".to_string(),
            project_id: None,
        };
        assert_eq!(msg.id(), "my-id");
    }

    #[test]
    fn test_wire_format_example() {
        let create = r#"{"id":"1","type":"create_session","session_id":"myapp_feature-auth","working_directory":"/tmp/wt","command":"claude","args":["--dangerously-skip-permissions"]}"#;
        let parsed: ClientMessage = serde_json::from_str(create).unwrap();
        assert_eq!(parsed.id(), "1");
        if let ClientMessage::CreateSession {
            session_id,
            command,
            args,
            ..
        } = parsed
        {
            assert_eq!(session_id, "myapp_feature-auth");
            assert_eq!(command, "claude");
            assert_eq!(args, vec!["--dangerously-skip-permissions"]);
        } else {
            panic!("wrong variant");
        }
    }
}
