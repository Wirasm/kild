use uuid::Uuid;

/// Whether this agent supports session resume via --session-id / --resume flags.
pub fn supports_resume(agent: &str) -> bool {
    agent == "claude"
}

/// Generate a new agent session ID (UUID v4).
pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}

/// Build extra CLI args to set a session ID on initial create.
/// Returns args to append to the agent command string.
/// Returns an empty vec for agents that don't support resume.
pub fn create_session_args(agent: &str, session_id: &str) -> Vec<String> {
    match agent {
        "claude" => vec!["--session-id".into(), session_id.into()],
        _ => vec![],
    }
}

/// Build extra CLI args to resume an existing session.
/// Returns args to append to the agent command string.
/// Returns an empty vec for agents that don't support resume.
pub fn resume_session_args(agent: &str, session_id: &str) -> Vec<String> {
    match agent {
        "claude" => vec!["--resume".into(), session_id.into()],
        _ => vec![],
    }
}

/// Generate a deterministic task list ID from a session ID.
///
/// Format: `kild-{sanitized_session_id}` — unique per kild, deterministic for the same session.
/// The session ID's `/` separator is replaced with `-` to produce a flat directory name
/// (Claude Code uses the ID directly as a directory under `~/.claude/tasks/`).
pub fn generate_task_list_id(session_id: &str) -> String {
    format!("kild-{}", session_id.replace('/', "-"))
}

/// Build env vars for task list transfer.
///
/// Returns env vars to inject for agents that support task list persistence.
/// Currently only Claude Code uses `CLAUDE_CODE_TASK_LIST_ID`.
/// Returns an empty vec for agents that don't support task lists.
pub fn task_list_env_vars(agent: &str, task_list_id: &str) -> Vec<(String, String)> {
    match agent {
        "claude" => vec![("CLAUDE_CODE_TASK_LIST_ID".into(), task_list_id.into())],
        _ => vec![],
    }
}

/// Build env vars for Codex agent sessions.
///
/// Returns `KILD_SESSION_BRANCH` so the Codex notify hook can identify
/// which kild session to report status for. This serves as a fallback when
/// `--self` PWD detection is unavailable (e.g., the hook runs from outside
/// the worktree directory).
/// Returns an empty vec for non-Codex agents.
pub fn codex_env_vars(agent: &str, branch: &str) -> Vec<(String, String)> {
    match agent {
        "codex" => vec![("KILD_SESSION_BRANCH".into(), branch.into())],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_resume_claude() {
        assert!(supports_resume("claude"));
    }

    #[test]
    fn test_supports_resume_other_agents() {
        for agent in &["kiro", "gemini", "codex", "amp", "opencode"] {
            assert!(
                !supports_resume(agent),
                "agent '{}' should not support resume",
                agent
            );
        }
    }

    #[test]
    fn test_generate_session_id_is_valid_uuid() {
        let id = generate_session_id();
        assert!(
            Uuid::parse_str(&id).is_ok(),
            "generated ID should be a valid UUID: {}",
            id
        );
    }

    #[test]
    fn test_generate_session_id_is_unique() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2, "two generated IDs should be different");
    }

    #[test]
    fn test_create_session_args_claude() {
        let args = create_session_args("claude", "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            args,
            vec!["--session-id", "550e8400-e29b-41d4-a716-446655440000"]
        );
    }

    #[test]
    fn test_create_session_args_other_agent() {
        let args = create_session_args("kiro", "550e8400-e29b-41d4-a716-446655440000");
        assert!(args.is_empty());
    }

    #[test]
    fn test_resume_session_args_claude() {
        let args = resume_session_args("claude", "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            args,
            vec!["--resume", "550e8400-e29b-41d4-a716-446655440000"]
        );
    }

    #[test]
    fn test_resume_session_args_other_agent() {
        let args = resume_session_args("kiro", "550e8400-e29b-41d4-a716-446655440000");
        assert!(args.is_empty());
    }

    #[test]
    fn test_generate_task_list_id() {
        let id = generate_task_list_id("myproject_my-feature");
        assert_eq!(id, "kild-myproject_my-feature");
    }

    #[test]
    fn test_generate_task_list_id_sanitizes_slash() {
        let id = generate_task_list_id("48fc8bd4db48d47e/my-branch");
        assert_eq!(id, "kild-48fc8bd4db48d47e-my-branch");
    }

    #[test]
    fn test_task_list_env_vars_claude() {
        let vars = task_list_env_vars("claude", "kild-myproject_my-feature");
        assert_eq!(
            vars,
            vec![(
                "CLAUDE_CODE_TASK_LIST_ID".to_string(),
                "kild-myproject_my-feature".to_string()
            )]
        );
    }

    #[test]
    fn test_task_list_env_vars_other_agents() {
        for agent in &["kiro", "gemini", "codex", "amp", "opencode"] {
            let vars = task_list_env_vars(agent, "kild-test");
            assert!(
                vars.is_empty(),
                "agent '{}' should not have task list env vars",
                agent
            );
        }
    }

    #[test]
    fn test_codex_env_vars_codex_agent() {
        let vars = codex_env_vars("codex", "my-feature");
        assert_eq!(
            vars,
            vec![("KILD_SESSION_BRANCH".to_string(), "my-feature".to_string())]
        );
    }

    #[test]
    fn test_codex_env_vars_other_agents() {
        for agent in &["claude", "kiro", "gemini", "amp", "opencode"] {
            let vars = codex_env_vars(agent, "my-branch");
            assert!(
                vars.is_empty(),
                "agent '{}' should not have codex env vars",
                agent
            );
        }
    }
}
