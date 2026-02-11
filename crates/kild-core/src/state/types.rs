use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::sessions::types::AgentStatus;

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

/// All business operations that can be dispatched through the store.
///
/// Each variant captures the parameters needed to execute the operation.
/// Commands use owned types (`String`, `PathBuf`) so they can be serialized,
/// stored, and sent across boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Create a new kild session with a git worktree and agent.
    CreateKild {
        /// Branch name for the new kild (will be prefixed with `kild/`).
        branch: String,
        /// What agent to launch (default, specific, or bare shell).
        agent_mode: AgentMode,
        /// Optional note describing what this kild is for.
        note: Option<String>,
        /// Project path for session tracking. Uses current directory if `None`.
        project_path: Option<PathBuf>,
    },
    /// Destroy a kild session, removing worktree and session file.
    DestroyKild {
        branch: String,
        /// Bypass safety checks (uncommitted changes, unpushed commits).
        force: bool,
    },
    /// Open an additional agent terminal in an existing kild (does not replace the current agent).
    OpenKild {
        branch: String,
        /// What to launch: default agent, specific agent, or bare shell.
        mode: OpenMode,
        /// Runtime mode explicitly requested via CLI flags.
        /// `None` = no flag passed; auto-detect from session's stored mode, then config.
        runtime_mode: Option<RuntimeMode>,
        /// Resume the previous agent conversation instead of starting fresh.
        #[serde(default)]
        resume: bool,
    },
    /// Stop the agent process in a kild without destroying it.
    StopKild { branch: String },
    /// Complete a kild: check if PR was merged, delete remote branch if merged, destroy session.
    /// Always blocks on uncommitted changes (use `kild destroy --force` for forced removal).
    CompleteKild { branch: String },
    /// Update agent status for a kild session.
    UpdateAgentStatus { branch: String, status: AgentStatus },
    /// Refresh PR status for a kild session from GitHub.
    RefreshPrStatus { branch: String },
    /// Refresh the session list from disk.
    RefreshSessions,
    /// Add a project to the project list. Name is derived from path if `None`.
    AddProject { path: PathBuf, name: Option<String> },
    /// Remove a project from the project list.
    RemoveProject { path: PathBuf },
    /// Select a project as active. `None` path means select all projects.
    SelectProject { path: Option<PathBuf> },
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_create_kild_with_bare_shell_serde() {
        let cmd = Command::CreateKild {
            branch: "debug-session".to_string(),
            agent_mode: AgentMode::BareShell,
            note: None,
            project_path: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, deserialized);
    }

    #[test]
    fn test_command_serde_roundtrip() {
        let cmd = Command::CreateKild {
            branch: "my-feature".to_string(),
            agent_mode: AgentMode::Agent("claude".to_string()),
            note: Some("Working on auth".to_string()),
            project_path: Some(PathBuf::from("/home/user/project")),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, deserialized);
    }

    #[test]
    fn test_all_command_variants_serialize() {
        let commands = vec![
            Command::CreateKild {
                branch: "feature".to_string(),
                agent_mode: AgentMode::Agent("claude".to_string()),
                note: None,
                project_path: None,
            },
            Command::DestroyKild {
                branch: "feature".to_string(),
                force: false,
            },
            Command::OpenKild {
                branch: "feature".to_string(),
                mode: OpenMode::DefaultAgent,
                runtime_mode: Some(RuntimeMode::Terminal),
                resume: false,
            },
            Command::OpenKild {
                branch: "feature".to_string(),
                mode: OpenMode::DefaultAgent,
                runtime_mode: None,
                resume: false,
            },
            Command::StopKild {
                branch: "feature".to_string(),
            },
            Command::CompleteKild {
                branch: "feature".to_string(),
            },
            Command::UpdateAgentStatus {
                branch: "feature".to_string(),
                status: AgentStatus::Working,
            },
            Command::RefreshPrStatus {
                branch: "feature".to_string(),
            },
            Command::RefreshSessions,
            Command::AddProject {
                path: PathBuf::from("/projects/app"),
                name: Some("App".to_string()),
            },
            Command::RemoveProject {
                path: PathBuf::from("/projects/app"),
            },
            Command::SelectProject {
                path: Some(PathBuf::from("/projects/app")),
            },
            Command::SelectProject { path: None },
        ];
        for cmd in commands {
            assert!(
                serde_json::to_string(&cmd).is_ok(),
                "Failed to serialize: {:?}",
                cmd
            );
        }
    }

    #[test]
    fn test_command_deserialize_all_variants() {
        let commands = vec![
            Command::CreateKild {
                branch: "test".to_string(),
                agent_mode: AgentMode::Agent("kiro".to_string()),
                note: Some("test note".to_string()),
                project_path: Some(PathBuf::from("/tmp/project")),
            },
            Command::DestroyKild {
                branch: "test".to_string(),
                force: true,
            },
            Command::OpenKild {
                branch: "test".to_string(),
                mode: OpenMode::Agent("gemini".to_string()),
                runtime_mode: Some(RuntimeMode::Terminal),
                resume: false,
            },
            Command::OpenKild {
                branch: "test".to_string(),
                mode: OpenMode::DefaultAgent,
                runtime_mode: None,
                resume: false,
            },
            Command::StopKild {
                branch: "test".to_string(),
            },
            Command::CompleteKild {
                branch: "test".to_string(),
            },
            Command::UpdateAgentStatus {
                branch: "feature".to_string(),
                status: AgentStatus::Working,
            },
            Command::RefreshPrStatus {
                branch: "feature".to_string(),
            },
            Command::RefreshSessions,
            Command::AddProject {
                path: PathBuf::from("/tmp"),
                name: Some("Tmp".to_string()),
            },
            Command::RemoveProject {
                path: PathBuf::from("/tmp"),
            },
            Command::SelectProject { path: None },
        ];

        for cmd in commands {
            let json = serde_json::to_string(&cmd).unwrap();
            let roundtripped: Command = serde_json::from_str(&json).unwrap();
            assert_eq!(cmd, roundtripped);
        }
    }

    #[test]
    fn test_open_kild_with_resume_true_serde_roundtrip() {
        let cmd = Command::OpenKild {
            branch: "feature-auth".to_string(),
            mode: OpenMode::DefaultAgent,
            runtime_mode: Some(RuntimeMode::Daemon),
            resume: true,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let roundtripped: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, roundtripped);

        // Verify the resume field is actually in the JSON
        assert!(json.contains("\"resume\":true"));
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
    fn test_runtime_mode_roundtrip_new_format() {
        for mode in [RuntimeMode::Terminal, RuntimeMode::Daemon] {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: RuntimeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }
}
