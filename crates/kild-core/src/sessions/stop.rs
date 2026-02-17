use tracing::{error, info, warn};

use crate::config::Config;
use crate::sessions::{errors::SessionError, persistence, types::*};
use crate::terminal;

/// Stops the agent process in a kild without destroying the kild.
///
/// The worktree and session file are preserved. The kild can be reopened with `open_session()`.
pub fn stop_session(name: &str) -> Result<(), SessionError> {
    info!(event = "core.session.stop_started", name = name);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let mut session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.stop_found",
        session_id = %session.id,
        branch = %session.branch
    );

    // 2. Close all terminal windows and kill all processes
    {
        if !session.has_agents() {
            warn!(
                event = "core.session.stop_no_agents",
                session_id = %session.id,
                branch = %session.branch,
                "Session has no tracked agents — skipping process/terminal cleanup"
            );
        }

        // Iterate all tracked agents — branch on daemon vs terminal
        let mut kill_errors: Vec<(u32, String)> = Vec::with_capacity(session.agent_count());
        for agent_proc in session.agents() {
            if let Some(daemon_sid) = agent_proc.daemon_session_id() {
                // Daemon-managed: destroy daemon session state via IPC.
                // We use destroy (not stop) because daemon session state is ephemeral —
                // it only exists while a PTY is alive. `kild open` will create a fresh
                // daemon session when reopening. Using stop would leave a stale entry
                // that blocks re-creation with the same spawn_id (#309).
                info!(
                    event = "core.session.destroy_daemon_session",
                    daemon_session_id = daemon_sid,
                    agent = agent_proc.agent()
                );
                if let Err(e) = crate::daemon::client::destroy_daemon_session(daemon_sid, false) {
                    error!(
                        event = "core.session.destroy_daemon_failed",
                        daemon_session_id = daemon_sid,
                        error = %e
                    );
                    kill_errors.push((0, e.to_string()));
                }
            } else {
                // Terminal-managed: close window + kill process
                if let (Some(terminal_type), Some(window_id)) =
                    (agent_proc.terminal_type(), agent_proc.terminal_window_id())
                {
                    info!(
                        event = "core.session.stop_close_terminal",
                        terminal_type = ?terminal_type,
                        agent = agent_proc.agent(),
                    );
                    terminal::handler::close_terminal(terminal_type, Some(window_id));
                }

                let Some(pid) = agent_proc.process_id() else {
                    continue;
                };

                info!(
                    event = "core.session.stop_kill_started",
                    pid = pid,
                    agent = agent_proc.agent()
                );

                let result = crate::process::kill_process(
                    pid,
                    agent_proc.process_name(),
                    agent_proc.process_start_time(),
                );

                match result {
                    Ok(()) => {
                        info!(event = "core.session.stop_kill_completed", pid = pid);
                    }
                    Err(crate::process::ProcessError::NotFound { .. }) => {
                        info!(event = "core.session.stop_kill_already_dead", pid = pid);
                    }
                    Err(e) => {
                        error!(event = "core.session.stop_kill_failed", pid = pid, error = %e);
                        kill_errors.push((pid, e.to_string()));
                    }
                }
            }
        }

        if !kill_errors.is_empty() {
            for (pid, err) in &kill_errors {
                error!(
                    event = "core.session.stop_kill_failed_summary",
                    pid = pid,
                    error = %err
                );
            }

            let error_count = kill_errors.len();
            let (first_pid, first_msg) = {
                let (p, m) = kill_errors.first().unwrap();
                (*p, m.clone())
            };

            let message = if error_count == 1 {
                first_msg
            } else {
                let pids: String = kill_errors
                    .iter()
                    .map(|(p, _)| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{} processes failed to stop (PIDs: {}). Kill them manually.",
                    error_count, pids
                )
            };

            return Err(SessionError::ProcessKillFailed {
                pid: first_pid,
                message,
            });
        }
    }

    // 3. Delete PID files so next open() won't read stale PIDs (best-effort)
    super::destroy::cleanup_session_pid_files(&session, config.kild_dir(), "stop");

    // 4. Backfill runtime_mode for sessions created before this field existed.
    // Infer from agents: if any agent has daemon_session_id, session was daemon-managed.
    if session.runtime_mode.is_none() {
        let has_daemon_agent = session
            .agents()
            .iter()
            .any(|a| a.daemon_session_id().is_some());

        let inferred_mode = if has_daemon_agent {
            crate::state::types::RuntimeMode::Daemon
        } else {
            crate::state::types::RuntimeMode::Terminal
        };

        session.runtime_mode = Some(inferred_mode);

        info!(
            event = "core.session.runtime_mode_inferred",
            session_id = %session.id,
            mode = ?session.runtime_mode,
            "Inferred runtime_mode from agent metadata"
        );
    }

    // 5. Remove agent status sidecar (best-effort, mirrors destroy.rs)
    persistence::remove_agent_status_file(&config.sessions_dir(), &session.id);

    // 6. Clear process info and set status to Stopped
    session.clear_agents();
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 7. Save updated session (keep worktree, keep session file)
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.stop_completed",
        session_id = %session.id
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_session_not_found() {
        let result = stop_session("non-existent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_stop_infers_runtime_mode_daemon_from_agent() {
        use crate::state::types::RuntimeMode;
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_stop_infer_daemon_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with runtime_mode: None but daemon_session_id on agent
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test-project_infer-daemon_0".to_string(),
            None,
            None,
            None,
            None,
            None,
            "claude --print".to_string(),
            chrono::Utc::now().to_rfc3339(),
            Some("daemon-session-123".to_string()),
        )
        .unwrap();

        let session = Session::new(
            "test-project_infer-daemon".into(),
            "test-project".into(),
            "infer-daemon".into(),
            worktree_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![agent],
            None,
            None,
            None, // runtime_mode: None — simulates old session
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save");

        // Simulate the inference logic from stop_session (without running real stop)
        let mut loaded = persistence::find_session_by_name(&sessions_dir, "infer-daemon")
            .expect("Failed to find")
            .expect("Session should exist");
        assert!(loaded.runtime_mode.is_none(), "Should start as None");

        // Apply the inference logic
        if loaded.runtime_mode.is_none() {
            let is_daemon = loaded
                .agents()
                .iter()
                .any(|a| a.daemon_session_id().is_some());
            loaded.runtime_mode = Some(if is_daemon {
                RuntimeMode::Daemon
            } else {
                RuntimeMode::Terminal
            });
        }
        loaded.clear_agents();
        loaded.status = SessionStatus::Stopped;
        persistence::save_session_to_file(&loaded, &sessions_dir).expect("Failed to save");

        // Reload and verify
        let reloaded = persistence::find_session_by_name(&sessions_dir, "infer-daemon")
            .expect("Failed to find")
            .expect("Session should exist");
        assert_eq!(
            reloaded.runtime_mode,
            Some(RuntimeMode::Daemon),
            "Should infer Daemon from agent with daemon_session_id"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stop_infers_runtime_mode_terminal_when_no_daemon() {
        use crate::state::types::RuntimeMode;
        use crate::terminal::types::TerminalType;
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_stop_infer_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with runtime_mode: None and terminal-based agent (no daemon_session_id)
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test-project_infer-terminal_0".to_string(),
            Some(99999),
            Some("fake-process".to_string()),
            Some(1234567890),
            Some(TerminalType::Ghostty),
            Some("test-window".to_string()),
            "claude --print".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None, // no daemon_session_id
        )
        .unwrap();

        let session = Session::new(
            "test-project_infer-terminal".into(),
            "test-project".into(),
            "infer-terminal".into(),
            worktree_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![agent],
            None,
            None,
            None, // runtime_mode: None
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save");

        let mut loaded = persistence::find_session_by_name(&sessions_dir, "infer-terminal")
            .expect("Failed to find")
            .expect("Session should exist");
        assert!(loaded.runtime_mode.is_none());

        // Apply inference logic
        if loaded.runtime_mode.is_none() {
            let is_daemon = loaded
                .agents()
                .iter()
                .any(|a| a.daemon_session_id().is_some());
            loaded.runtime_mode = Some(if is_daemon {
                RuntimeMode::Daemon
            } else {
                RuntimeMode::Terminal
            });
        }
        loaded.clear_agents();
        loaded.status = SessionStatus::Stopped;
        persistence::save_session_to_file(&loaded, &sessions_dir).expect("Failed to save");

        let reloaded = persistence::find_session_by_name(&sessions_dir, "infer-terminal")
            .expect("Failed to find")
            .expect("Session should exist");
        assert_eq!(
            reloaded.runtime_mode,
            Some(RuntimeMode::Terminal),
            "Should infer Terminal when no agent has daemon_session_id"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stop_preserves_existing_runtime_mode() {
        use crate::state::types::RuntimeMode;
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_stop_preserve_mode_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with runtime_mode already set — should NOT be re-inferred
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test-project_preserve-mode_0".to_string(),
            None,
            None,
            None,
            None,
            None,
            "claude --print".to_string(),
            chrono::Utc::now().to_rfc3339(),
            Some("daemon-session-456".to_string()), // daemon agent
        )
        .unwrap();

        let session = Session::new(
            "test-project_preserve-mode".into(),
            "test-project".into(),
            "preserve-mode".into(),
            worktree_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![agent],
            None,
            None,
            Some(RuntimeMode::Daemon), // already set
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save");

        let mut loaded = persistence::find_session_by_name(&sessions_dir, "preserve-mode")
            .expect("Failed to find")
            .expect("Session should exist");
        assert_eq!(loaded.runtime_mode, Some(RuntimeMode::Daemon));

        // The inference should NOT run because runtime_mode is already Some
        if loaded.runtime_mode.is_none() {
            panic!("Should not enter inference block when runtime_mode is already set");
        }
        loaded.clear_agents();
        loaded.status = SessionStatus::Stopped;
        persistence::save_session_to_file(&loaded, &sessions_dir).expect("Failed to save");

        let reloaded = persistence::find_session_by_name(&sessions_dir, "preserve-mode")
            .expect("Failed to find")
            .expect("Session should exist");
        assert_eq!(
            reloaded.runtime_mode,
            Some(RuntimeMode::Daemon),
            "Should preserve existing runtime_mode without re-inference"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stop_session_clears_process_info_and_sets_stopped_status() {
        use crate::terminal::types::TerminalType;
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_stop_state_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with Active status and process info in agent
        let agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_stop-test_0".to_string(),
            Some(99999), // Fake PID that won't exist
            Some("fake-process".to_string()),
            Some(1234567890),
            Some(TerminalType::Ghostty),
            Some("test-window".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None,
        )
        .unwrap();
        let session = Session::new(
            "test-project_stop-test".into(),
            "test-project".into(),
            "stop-test".into(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![agent],
            None,
            None,
            None,
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session exists with Active status
        let before = persistence::find_session_by_name(&sessions_dir, "stop-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(before.status, SessionStatus::Active);
        assert!(before.has_agents());

        // Simulate stop by directly updating session (avoids process kill complexity)
        let mut stopped_session = before;
        stopped_session.clear_agents();
        stopped_session.status = SessionStatus::Stopped;
        stopped_session.last_activity = Some(chrono::Utc::now().to_rfc3339());
        persistence::save_session_to_file(&stopped_session, &sessions_dir)
            .expect("Failed to save stopped session");

        // Verify state changes persisted
        let after = persistence::find_session_by_name(&sessions_dir, "stop-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(
            after.status,
            SessionStatus::Stopped,
            "Status should be Stopped"
        );
        assert!(!after.has_agents(), "agents should be cleared");
        // Worktree should still exist
        assert!(worktree_dir.exists(), "Worktree should be preserved");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stop_removes_agent_status_sidecar() {
        use crate::sessions::types::{AgentStatus, AgentStatusInfo};
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_stop_sidecar_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session
        let session = Session::new(
            "test-project_sidecar-test".into(),
            "test-project".into(),
            "sidecar-test".into(),
            worktree_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            None,
            None,
            None,
        );
        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save");

        // Write agent status sidecar file
        let status_info = AgentStatusInfo {
            status: AgentStatus::Working,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        persistence::write_agent_status(&sessions_dir, &session.id, &status_info)
            .expect("Failed to write status");

        // Verify sidecar exists
        let sidecar_file = sessions_dir
            .join("test-project_sidecar-test")
            .join("status");
        assert!(sidecar_file.exists(), "Sidecar should exist before stop");
        assert!(
            persistence::read_agent_status(&sessions_dir, &session.id).is_some(),
            "Should read agent status before stop"
        );

        // Simulate stop: remove sidecar + clear agents + set stopped
        persistence::remove_agent_status_file(&sessions_dir, &session.id);
        let mut stopped = session;
        stopped.clear_agents();
        stopped.status = SessionStatus::Stopped;
        persistence::save_session_to_file(&stopped, &sessions_dir).expect("Failed to save");

        // Verify sidecar is gone
        assert!(
            !sidecar_file.exists(),
            "Sidecar should be removed after stop"
        );
        assert!(
            persistence::read_agent_status(&sessions_dir, &stopped.id).is_none(),
            "Should return None for agent status after stop"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
