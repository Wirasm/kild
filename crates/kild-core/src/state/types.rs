use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// All business operations that can be dispatched through the store.
///
/// Each variant captures the parameters needed to execute the operation.
/// Commands use owned types (`String`, `PathBuf`) so they can be serialized,
/// stored, and sent across boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Create a new kild session with a git worktree and agent.
    CreateKild {
        /// Branch name for the new kild (will be prefixed with `kild_`).
        branch: String,
        /// Agent to launch. Uses default from config if `None`.
        agent: Option<String>,
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
        /// Agent to launch. Uses default from config if `None`.
        agent: Option<String>,
    },
    /// Stop the agent process in a kild without destroying it.
    StopKild { branch: String },
    /// Complete a kild: check if PR was merged, delete remote branch if merged, destroy session.
    /// The `force` flag bypasses safety checks for session destruction only.
    CompleteKild { branch: String, force: bool },
    /// Refresh the session list from disk.
    RefreshSessions,
    /// Add a project to the project list.
    AddProject { path: PathBuf, name: String },
    /// Remove a project from the project list.
    RemoveProject { path: PathBuf },
    /// Select a project as active. `None` path means select all projects.
    SelectProject { path: Option<PathBuf> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_serde_roundtrip() {
        let cmd = Command::CreateKild {
            branch: "my-feature".to_string(),
            agent: Some("claude".to_string()),
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
                agent: Some("claude".to_string()),
                note: None,
                project_path: None,
            },
            Command::DestroyKild {
                branch: "feature".to_string(),
                force: false,
            },
            Command::OpenKild {
                branch: "feature".to_string(),
                agent: None,
            },
            Command::StopKild {
                branch: "feature".to_string(),
            },
            Command::CompleteKild {
                branch: "feature".to_string(),
                force: true,
            },
            Command::RefreshSessions,
            Command::AddProject {
                path: PathBuf::from("/projects/app"),
                name: "App".to_string(),
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
                agent: Some("kiro".to_string()),
                note: Some("test note".to_string()),
                project_path: Some(PathBuf::from("/tmp/project")),
            },
            Command::DestroyKild {
                branch: "test".to_string(),
                force: true,
            },
            Command::OpenKild {
                branch: "test".to_string(),
                agent: Some("gemini".to_string()),
            },
            Command::StopKild {
                branch: "test".to_string(),
            },
            Command::CompleteKild {
                branch: "test".to_string(),
                force: false,
            },
            Command::RefreshSessions,
            Command::AddProject {
                path: PathBuf::from("/tmp"),
                name: "Tmp".to_string(),
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
}
