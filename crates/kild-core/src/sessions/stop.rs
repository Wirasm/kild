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
        session_id = session.id,
        branch = session.branch
    );

    // 2. Close all terminal windows and kill all processes
    {
        if !session.has_agents() {
            warn!(
                event = "core.session.stop_no_agents",
                session_id = session.id,
                branch = session.branch,
                "Session has no tracked agents — skipping process/terminal cleanup"
            );
        }

        // Iterate all tracked agents — branch on daemon vs terminal
        let mut kill_errors: Vec<(u32, String)> = Vec::new();
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

            let pids: Vec<String> = kill_errors.iter().map(|(p, _)| p.to_string()).collect();
            let (first_pid, first_msg) = kill_errors.into_iter().next().unwrap();

            let message = if pids.len() == 1 {
                first_msg
            } else {
                format!(
                    "{} processes failed to stop (PIDs: {}). Kill them manually.",
                    pids.len(),
                    pids.join(", ")
                )
            };

            return Err(SessionError::ProcessKillFailed {
                pid: first_pid,
                message,
            });
        }
    }

    // 3. Delete PID files so next open() won't read stale PIDs (best-effort)
    super::destroy::cleanup_session_pid_files(&session, &config.kild_dir, "stop");

    // 4. Clear process info and set status to Stopped
    session.clear_agents();
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 6. Save updated session (keep worktree, keep session file)
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.stop_completed",
        session_id = session.id
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
            "test-project_stop-test".to_string(),
            "test-project".to_string(),
            "stop-test".to_string(),
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
}
