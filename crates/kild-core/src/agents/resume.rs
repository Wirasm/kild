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
}
