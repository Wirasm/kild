use crate::sessions::{errors::SessionError, types::*};
use std::fs;
use std::path::Path;

pub fn validate_session_request(
    name: &str,
    command: &str,
    agent: &str,
) -> Result<ValidatedRequest, SessionError> {
    if name.trim().is_empty() {
        return Err(SessionError::InvalidName);
    }

    if command.trim().is_empty() {
        return Err(SessionError::InvalidCommand);
    }

    Ok(ValidatedRequest {
        name: name.trim().to_string(),
        command: command.trim().to_string(),
        agent: agent.to_string(),
    })
}

pub fn generate_session_id(project_id: &str, branch: &str) -> String {
    format!("{}/{}", project_id, branch)
}

pub fn calculate_port_range(session_index: u32) -> (u16, u16) {
    let base_port = 3000u16 + (session_index as u16 * 100);
    (base_port, base_port + 99)
}

pub fn get_agent_command(agent: &str) -> String {
    match agent {
        "claude" => "cc".to_string(),
        "kiro" => "kiro-cli".to_string(),
        "gemini" => "gemini --yolo".to_string(),
        "codex" => "codex --dangerously-bypass-approvals-and-sandbox".to_string(),
        _ => agent.to_string(), // Use as-is for custom agents
    }
}

pub fn validate_branch_name(branch: &str) -> Result<String, SessionError> {
    let trimmed = branch.trim();

    if trimmed.is_empty() {
        return Err(SessionError::InvalidName);
    }

    // Basic git branch name validation
    if trimmed.contains("..") || trimmed.starts_with('-') || trimmed.contains(' ') {
        return Err(SessionError::InvalidName);
    }

    Ok(trimmed.to_string())
}

pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    if !sessions_dir.exists() {
        fs::create_dir_all(sessions_dir)
            .map_err(|e| SessionError::IoError { source: e })?;
    }
    Ok(())
}

pub fn save_session_to_file(session: &Session, sessions_dir: &Path) -> Result<(), SessionError> {
    let session_file = sessions_dir.join(format!("{}.json", session.id.replace('/', "_")));
    let session_json = serde_json::to_string_pretty(session)
        .map_err(|e| SessionError::IoError { source: std::io::Error::new(std::io::ErrorKind::InvalidData, e) })?;
    
    // Write atomically by writing to temp file first, then renaming
    let temp_file = session_file.with_extension("json.tmp");
    fs::write(&temp_file, session_json)?;
    fs::rename(&temp_file, &session_file)?;
    
    Ok(())
}

pub fn load_sessions_from_files(sessions_dir: &Path) -> Result<Vec<Session>, SessionError> {
    let mut sessions = Vec::new();
    
    // Return empty list if sessions directory doesn't exist
    if !sessions_dir.exists() {
        return Ok(sessions);
    }
    
    let entries = fs::read_dir(sessions_dir)
        .map_err(|e| SessionError::IoError { source: e })?;
    
    for entry in entries {
        let entry = entry.map_err(|e| SessionError::IoError { source: e })?;
        let path = entry.path();
        
        // Only process .json files
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let content = fs::read_to_string(&path)
                .map_err(|e| SessionError::IoError { source: e })?;
            
            match serde_json::from_str::<Session>(&content) {
                Ok(session) => sessions.push(session),
                Err(_) => {
                    // Skip invalid JSON files silently for now
                    // TODO: Add warning log in US-005
                    continue;
                }
            }
        }
    }
    
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_session_request_success() {
        let result = validate_session_request("test", "echo hello", "claude");
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.name, "test");
        assert_eq!(validated.command, "echo hello");
        assert_eq!(validated.agent, "claude");
    }

    #[test]
    fn test_validate_session_request_empty_name() {
        let result = validate_session_request("", "echo hello", "claude");
        assert!(matches!(result, Err(SessionError::InvalidName)));
    }

    #[test]
    fn test_validate_session_request_empty_command() {
        let result = validate_session_request("test", "", "claude");
        assert!(matches!(result, Err(SessionError::InvalidCommand)));
    }

    #[test]
    fn test_validate_session_request_whitespace() {
        let result = validate_session_request("  test  ", "  echo hello  ", "claude");
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.name, "test");
        assert_eq!(validated.command, "echo hello");
    }

    #[test]
    fn test_generate_session_id() {
        let id = generate_session_id("my-project", "feature-branch");
        assert_eq!(id, "my-project/feature-branch");
    }

    #[test]
    fn test_calculate_port_range() {
        assert_eq!(calculate_port_range(0), (3000, 3099));
        assert_eq!(calculate_port_range(1), (3100, 3199));
        assert_eq!(calculate_port_range(5), (3500, 3599));
    }

    #[test]
    fn test_get_agent_command() {
        assert_eq!(get_agent_command("claude"), "cc");
        assert_eq!(get_agent_command("kiro"), "kiro-cli");
        assert_eq!(get_agent_command("gemini"), "gemini --yolo");
        assert_eq!(
            get_agent_command("codex"),
            "codex --dangerously-bypass-approvals-and-sandbox"
        );
        assert_eq!(get_agent_command("custom"), "custom");
    }

    #[test]
    fn test_validate_branch_name() {
        assert!(validate_branch_name("feature-branch").is_ok());
        assert!(validate_branch_name("feat/auth").is_ok());

        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("  ").is_err());
        assert!(validate_branch_name("branch..name").is_err());
        assert!(validate_branch_name("-branch").is_err());
        assert!(validate_branch_name("branch name").is_err());
    }

    #[test]
    fn test_ensure_sessions_directory() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_sessions");
        
        // Clean up if exists
        let _ = std::fs::remove_dir_all(&temp_dir);
        
        // Should create directory
        assert!(ensure_sessions_directory(&temp_dir).is_ok());
        assert!(temp_dir.exists());
        
        // Should not error if directory already exists
        assert!(ensure_sessions_directory(&temp_dir).is_ok());
        
        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_to_file() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_save_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        // Save session
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Check file exists with correct name (/ replaced with _)
        let session_file = temp_dir.join("test_branch.json");
        assert!(session_file.exists());

        // Verify content
        let content = std::fs::read_to_string(&session_file).unwrap();
        let loaded_session: Session = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded_session, session);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_from_files() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_load_sessions");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Test empty directory
        let sessions = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);

        // Create test sessions
        let session1 = Session {
            id: "test/branch1".to_string(),
            project_id: "test".to_string(),
            branch: "branch1".to_string(),
            worktree_path: PathBuf::from("/tmp/test1"),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let session2 = Session {
            id: "test/branch2".to_string(),
            project_id: "test".to_string(),
            branch: "branch2".to_string(),
            worktree_path: PathBuf::from("/tmp/test2"),
            agent: "kiro".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2024-01-02T00:00:00Z".to_string(),
        };

        // Save sessions
        save_session_to_file(&session1, &temp_dir).unwrap();
        save_session_to_file(&session2, &temp_dir).unwrap();

        // Load sessions
        let loaded_sessions = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(loaded_sessions.len(), 2);

        // Verify sessions (order might vary)
        let ids: Vec<String> = loaded_sessions.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains(&"test/branch1".to_string()));
        assert!(ids.contains(&"test/branch2".to_string()));

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_nonexistent_directory() {
        use std::env;

        let nonexistent_dir = env::temp_dir().join("shards_test_nonexistent");
        let _ = std::fs::remove_dir_all(&nonexistent_dir);

        let sessions = load_sessions_from_files(&nonexistent_dir).unwrap();
        assert_eq!(sessions.len(), 0);
    }
}
