use tracing::{debug, error, info, warn};

use crate::agents;
use crate::config::{Config, KildConfig};
use crate::sessions::{errors::SessionError, persistence, types::*};
use crate::terminal;
use crate::terminal::types::SpawnResult;

use super::daemon_helpers::{
    build_daemon_create_request, compute_spawn_id, setup_claude_integration,
    setup_codex_integration, setup_opencode_integration, spawn_and_save_attach_window,
};

/// Resolve the effective runtime mode for `open_session`.
///
/// Priority: explicit CLI flag > session's stored mode > config > Terminal default.
/// Returns the resolved mode and its source label for logging.
fn resolve_effective_runtime_mode(
    explicit: Option<crate::state::types::RuntimeMode>,
    from_session: Option<crate::state::types::RuntimeMode>,
    config: &crate::config::KildConfig,
) -> (crate::state::types::RuntimeMode, &'static str) {
    if let Some(mode) = explicit {
        return (mode, "explicit");
    }
    if let Some(mode) = from_session {
        return (mode, "session");
    }
    if config.is_daemon_enabled() {
        (crate::state::types::RuntimeMode::Daemon, "config")
    } else {
        (crate::state::types::RuntimeMode::Terminal, "default")
    }
}

/// Capture process metadata from spawn result for PID reuse protection.
///
/// Attempts to get fresh process info from the OS. Falls back to spawn result metadata
/// if process info retrieval fails (logs warning in that case).
fn capture_process_metadata(
    spawn_result: &SpawnResult,
    event_prefix: &str,
) -> (Option<String>, Option<u64>) {
    let Some(pid) = spawn_result.process_id else {
        return (
            spawn_result.process_name.clone(),
            spawn_result.process_start_time,
        );
    };

    match crate::process::get_process_info(pid) {
        Ok(info) => (Some(info.name), Some(info.start_time)),
        Err(e) => {
            warn!(
                event = %format!("core.session.{}_process_info_failed", event_prefix),
                pid = pid,
                error = %e,
                "Failed to get process metadata after spawn - using spawn result metadata"
            );
            (
                spawn_result.process_name.clone(),
                spawn_result.process_start_time,
            )
        }
    }
}

/// Opens a new agent terminal in an existing kild (additive - doesn't close existing terminals).
///
/// This is the preferred way to add agents to a kild. Unlike restart, this does NOT
/// close existing terminals - multiple agents can run in the same kild.
///
/// The `runtime_mode` parameter overrides the runtime mode. Pass `None` to auto-detect
/// from the session's stored mode, then config, then Terminal default.
pub fn open_session(
    name: &str,
    mode: crate::state::types::OpenMode,
    runtime_mode: Option<crate::state::types::RuntimeMode>,
    resume: bool,
    yolo: bool,
) -> Result<Session, SessionError> {
    info!(
        event = "core.session.open_started",
        name = name,
        mode = ?mode,
        yolo = yolo
    );

    let config = Config::new();
    let kild_config = match KildConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            // Notify user via stderr - this is a developer tool, they need to know
            eprintln!("Warning: Config load failed ({}). Using defaults.", e);
            eprintln!("         Check ~/.kild/config.toml for syntax errors.");
            warn!(
                event = "core.config.load_failed",
                error = %e,
                "Config load failed during open, using defaults"
            );
            KildConfig::default()
        }
    };

    // 1. Find session by name (branch name)
    let mut session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.open_found",
        session_id = %session.id,
        branch = %session.branch
    );

    // 2. Verify worktree still exists
    if !session.worktree_path.exists() {
        return Err(SessionError::WorktreeNotFound {
            path: session.worktree_path.clone(),
        });
    }

    // 3. Determine agent and command based on OpenMode
    let is_bare_shell = matches!(mode, crate::state::types::OpenMode::BareShell);
    let (agent, agent_command) =
        match mode {
            crate::state::types::OpenMode::BareShell => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| {
                    let fallback = "/bin/sh".to_string();
                    warn!(
                        event = "core.session.shell_env_missing",
                        fallback = %fallback,
                        "$SHELL not set, falling back to /bin/sh"
                    );
                    fallback
                });
                info!(event = "core.session.open_shell_selected", shell = %shell);
                ("shell".to_string(), shell)
            }
            crate::state::types::OpenMode::Agent(name) => {
                info!(event = "core.session.open_agent_selected", agent = name);

                // Warn if agent CLI is not available in PATH
                if let Some(false) = agents::is_agent_available(&name) {
                    warn!(
                        event = "core.session.agent_not_available",
                        agent = %name,
                        session_id = %session.id,
                        "Agent CLI '{}' not found in PATH - session may fail to start",
                        name
                    );
                }

                let command = kild_config.get_agent_command(&name).map_err(|e| {
                    SessionError::ConfigError {
                        message: e.to_string(),
                    }
                })?;
                (name, command)
            }
            crate::state::types::OpenMode::DefaultAgent => {
                // Use session's stored agent, but fall back to config default
                // when the session was created with --no-agent (stored as "shell").
                // "shell" is not a registered agent, so get_agent_command would fail.
                let agent = if session.agent == "shell" {
                    let default = kild_config.agent.default.clone();
                    info!(
                        event = "core.session.open_agent_fallback_to_config",
                        stored_agent = "shell",
                        config_default = %default,
                        "Session was created with --no-agent, falling back to config default"
                    );
                    default
                } else {
                    session.agent.clone()
                };
                info!(event = "core.session.open_agent_selected", agent = agent);

                // Warn if agent CLI is not available in PATH
                if let Some(false) = agents::is_agent_available(&agent) {
                    warn!(
                        event = "core.session.agent_not_available",
                        agent = %agent,
                        session_id = %session.id,
                        "Agent CLI '{}' not found in PATH - session may fail to start",
                        agent
                    );
                }

                let command = kild_config.get_agent_command(&agent).map_err(|e| {
                    SessionError::ConfigError {
                        message: e.to_string(),
                    }
                })?;
                (agent, command)
            }
        };

    // 3b. Inject yolo flags into agent command
    let agent_command = if yolo && !is_bare_shell {
        if let Some(yolo_flags) = agents::get_yolo_flags(&agent) {
            info!(
                event = "core.session.yolo_flags_injected",
                agent = %agent,
                flags = yolo_flags
            );
            format!("{} {}", agent_command, yolo_flags)
        } else {
            warn!(
                event = "core.session.yolo_not_supported",
                agent = %agent,
                "Agent does not support --yolo mode"
            );
            eprintln!(
                "Warning: Agent '{}' does not support --yolo mode. Ignoring.",
                agent
            );
            agent_command
        }
    } else {
        agent_command
    };

    // 4. Apply resume / session-id logic to agent command
    let (agent_command, new_agent_session_id) = if resume && !is_bare_shell {
        if let Some(ref sid) = session.agent_session_id {
            if agents::resume::supports_resume(&agent) {
                let extra = agents::resume::resume_session_args(&agent, sid);
                let cmd = format!("{} {}", agent_command, extra.join(" "));
                info!(event = "core.session.resume_started", session_id = %sid, agent = %agent);
                (cmd, Some(sid.clone()))
            } else {
                error!(event = "core.session.resume_unsupported", agent = %agent);
                return Err(SessionError::ResumeUnsupported {
                    agent: agent.clone(),
                });
            }
        } else {
            error!(event = "core.session.resume_no_session_id", branch = name);
            return Err(SessionError::ResumeNoSessionId {
                branch: name.to_string(),
            });
        }
    } else if !is_bare_shell && agents::resume::supports_resume(&agent) {
        // Fresh open: generate new session ID for future resume capability
        let sid = agents::resume::generate_session_id();
        let extra = agents::resume::create_session_args(&agent, &sid);
        let cmd = if extra.is_empty() {
            agent_command
        } else {
            info!(event = "core.session.agent_session_id_set", session_id = %sid);
            format!("{} {}", agent_command, extra.join(" "))
        };
        (cmd, Some(sid))
    } else {
        (agent_command, None)
    };

    // 4b. Determine task list ID for agents that support it
    let new_task_list_id = if resume && !is_bare_shell {
        // Resume: reuse existing task_list_id so tasks persist
        session.task_list_id.clone()
    } else if !is_bare_shell && agents::resume::supports_resume(&agent) {
        // Fresh open: generate new task_list_id for a clean task list
        let tlid = agents::resume::generate_task_list_id(&session.id);
        info!(event = "core.session.task_list_id_set", task_list_id = %tlid);
        Some(tlid)
    } else {
        None
    };

    // 5. Spawn NEW agent — branch on whether session was daemon-managed
    let spawn_index = session.agent_count();
    let spawn_id = compute_spawn_id(&session.id, spawn_index);
    info!(
        event = "core.session.open_spawn_started",
        worktree = %session.worktree_path.display(),
        spawn_id = %spawn_id
    );

    let (effective_runtime_mode, source) =
        resolve_effective_runtime_mode(runtime_mode, session.runtime_mode.clone(), &kild_config);

    info!(
        event = "core.session.open_runtime_mode_resolved",
        mode = ?effective_runtime_mode,
        source = source
    );

    let use_daemon = effective_runtime_mode == crate::state::types::RuntimeMode::Daemon;

    let now = chrono::Utc::now().to_rfc3339();

    let new_agent = if use_daemon {
        // Auto-start daemon if not running (config.daemon.auto_start, default: true)
        crate::daemon::ensure_daemon_running(&kild_config)?;

        setup_codex_integration(&agent);
        setup_opencode_integration(&agent, &session.worktree_path);
        setup_claude_integration(&agent);

        // Daemon path: create new daemon PTY (uses shared helper with create_session)
        let (cmd, cmd_args, env_vars, use_login_shell) = build_daemon_create_request(
            &agent_command,
            &agent,
            &session.id,
            new_task_list_id.as_deref(),
            &session.branch,
        )?;

        let daemon_request = crate::daemon::client::DaemonCreateRequest {
            request_id: &spawn_id,
            session_id: &spawn_id,
            working_directory: &session.worktree_path,
            command: &cmd,
            args: &cmd_args,
            env_vars: &env_vars,
            rows: 24,
            cols: 80,
            use_login_shell,
        };
        let daemon_result =
            crate::daemon::client::create_pty_session(&daemon_request).map_err(|e| {
                SessionError::DaemonError {
                    message: e.to_string(),
                }
            })?;

        // Early exit detection: wait briefly, then verify PTY is still alive.
        // Fast-failing processes (bad resume session, missing binary, env issues)
        // typically exit within 200ms of spawn.
        std::thread::sleep(std::time::Duration::from_millis(200));

        if let Ok(Some((status, exit_code))) =
            crate::daemon::client::get_session_info(&daemon_result.daemon_session_id)
            && status == kild_protocol::SessionStatus::Stopped
        {
            let scrollback_tail =
                match crate::daemon::client::read_scrollback(&daemon_result.daemon_session_id) {
                    Ok(Some(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let lines: Vec<&str> = text.lines().collect();
                        let start = lines.len().saturating_sub(20);
                        lines[start..].join("\n")
                    }
                    Ok(None) => {
                        warn!(
                            event = "core.session.open_scrollback_empty",
                            daemon_session_id = %daemon_result.daemon_session_id,
                            "Daemon session exited early with empty scrollback"
                        );
                        String::new()
                    }
                    Err(e) => {
                        warn!(
                            event = "core.session.open_scrollback_read_failed",
                            daemon_session_id = %daemon_result.daemon_session_id,
                            error = %e,
                            "Failed to read scrollback after early PTY exit"
                        );
                        String::new()
                    }
                };

            if let Err(e) = crate::daemon::client::destroy_daemon_session(
                &daemon_result.daemon_session_id,
                true,
            ) {
                warn!(
                    event = "core.session.open_daemon_cleanup_failed",
                    daemon_session_id = %daemon_result.daemon_session_id,
                    error = %e,
                    "Failed to clean up daemon session after early exit"
                );
            }

            return Err(SessionError::DaemonPtyExitedEarly {
                exit_code,
                scrollback_tail,
            });
        }

        AgentProcess::new(
            agent.clone(),
            spawn_id,
            None,
            None,
            None,
            None,
            None,
            agent_command.clone(),
            now.clone(),
            Some(daemon_result.daemon_session_id),
        )?
    } else {
        setup_codex_integration(&agent);
        setup_opencode_integration(&agent, &session.worktree_path);
        setup_claude_integration(&agent);

        // Terminal path: spawn in external terminal
        // Wrap with `env` to strip nesting-detection vars and inject agent env.
        let terminal_command = {
            let mut env_prefix: Vec<(String, String)> = Vec::new();
            if let Some(ref tlid) = new_task_list_id {
                env_prefix.extend(agents::resume::task_list_env_vars(&agent, tlid));
            }
            env_prefix.extend(agents::resume::codex_env_vars(&agent, &session.branch));
            env_prefix.extend(agents::resume::claude_env_vars(&agent, &session.branch));
            super::env_cleanup::build_env_command(&env_prefix, &agent_command)
        };
        debug!(
            event = "core.session.terminal_command_constructed",
            command = %terminal_command,
        );
        let spawn_result = terminal::handler::spawn_terminal(
            &session.worktree_path,
            &terminal_command,
            &kild_config,
            Some(&spawn_id),
            Some(config.kild_dir()),
        )
        .map_err(|e| SessionError::TerminalError { source: e })?;

        let (process_name, process_start_time) = capture_process_metadata(&spawn_result, "open");

        let command = if spawn_result.command_executed.trim().is_empty() {
            format!("{} (command not captured)", agent)
        } else {
            spawn_result.command_executed.clone()
        };

        AgentProcess::new(
            agent.clone(),
            spawn_id,
            spawn_result.process_id,
            process_name.clone(),
            process_start_time,
            Some(spawn_result.terminal_type.clone()),
            spawn_result.terminal_window_id.clone(),
            command,
            now.clone(),
            None,
        )?
    };

    session.status = SessionStatus::Active;
    session.last_activity = Some(now);
    session.add_agent(new_agent);

    // Update agent session ID for resume support
    if let Some(sid) = new_agent_session_id {
        session.agent_session_id = Some(sid);
    }

    // Update task list ID for task list persistence
    if let Some(tlid) = new_task_list_id {
        session.task_list_id = Some(tlid);
    }

    // Update runtime mode so future opens auto-detect correctly
    let is_daemon = effective_runtime_mode == crate::state::types::RuntimeMode::Daemon;
    session.runtime_mode = Some(effective_runtime_mode);

    // 6. Save session BEFORE spawning attach window so `kild attach` can find it
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    // 7. Spawn attach window (best-effort) and update session with terminal info
    if is_daemon {
        spawn_and_save_attach_window(&mut session, name, &kild_config, &config.sessions_dir())?;
    }

    info!(
        event = "core.session.open_completed",
        session_id = %session.id,
        agent_count = session.agent_count()
    );

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_session_not_found() {
        let result = open_session(
            "non-existent",
            crate::state::types::OpenMode::DefaultAgent,
            Some(crate::state::types::RuntimeMode::Terminal),
            false,
            false,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    // --- Session Resume tests ---

    /// Tests the resume decision logic that open_session uses internally.
    ///
    /// These test the exact branching conditions from the resume logic
    /// without needing terminal/daemon infrastructure.
    #[test]
    fn test_resume_decision_unsupported_agent_with_session_id() {
        // Scenario: resume=true, agent=kiro (unsupported), session has session_id
        // Expected: ResumeUnsupported error
        use crate::errors::KildError;

        let agent = "kiro";
        let session_has_id = true;
        let resume = true;
        let is_bare_shell = false;

        // This replicates the decision logic in open_session
        if resume && !is_bare_shell {
            if session_has_id {
                if agents::resume::supports_resume(agent) {
                    panic!("kiro should not support resume");
                } else {
                    // This is the path that should produce ResumeUnsupported
                    let error = SessionError::ResumeUnsupported {
                        agent: agent.to_string(),
                    };
                    assert_eq!(error.error_code(), "RESUME_UNSUPPORTED");
                    assert!(error.to_string().contains("kiro"));
                }
            } else {
                panic!("session_has_id should be true in this test");
            }
        } else {
            panic!("resume && !is_bare_shell should be true");
        }
    }

    #[test]
    fn test_resume_decision_no_session_id() {
        // Scenario: resume=true, agent=claude (supported), session has NO session_id
        // Expected: ResumeNoSessionId error
        use crate::errors::KildError;

        let resume = true;
        let is_bare_shell = false;
        let session_has_id = false;

        if resume && !is_bare_shell {
            if session_has_id {
                panic!("session_has_id should be false in this test");
            } else {
                // This is the path that should produce ResumeNoSessionId
                let error = SessionError::ResumeNoSessionId {
                    branch: "my-feature".to_string(),
                };
                assert_eq!(error.error_code(), "RESUME_NO_SESSION_ID");
                assert!(error.to_string().contains("my-feature"));
            }
        } else {
            panic!("resume && !is_bare_shell should be true");
        }
    }

    #[test]
    fn test_resume_decision_agent_switch_to_unsupported() {
        // Scenario: Session created with Claude + session_id, user opens with --agent kiro --resume
        // The agent variable at decision point will be "kiro" (from OpenMode::Agent)
        // Expected: ResumeUnsupported because kiro doesn't support resume
        use crate::errors::KildError;

        let agent = "kiro"; // User switched agent
        let resume = true;
        let is_bare_shell = false;
        let session_agent_session_id = Some("550e8400-e29b-41d4-a716-446655440000");

        if resume && !is_bare_shell {
            if session_agent_session_id.is_some() {
                // The key check: even though session HAS a session_id,
                // the NEW agent (kiro) doesn't support resume
                assert!(
                    !agents::resume::supports_resume(agent),
                    "kiro should not support resume"
                );
                // → ResumeUnsupported error
                let error = SessionError::ResumeUnsupported {
                    agent: agent.to_string(),
                };
                assert!(error.is_user_error());
            } else {
                panic!("session should have id");
            }
        }
    }

    #[test]
    fn test_resume_decision_happy_path_claude() {
        // Scenario: resume=true, agent=claude, session has session_id
        // Expected: resume args generated, same session_id preserved

        let agent = "claude";
        let sid = "550e8400-e29b-41d4-a716-446655440000";
        let resume = true;
        let is_bare_shell = false;

        if resume && !is_bare_shell {
            assert!(agents::resume::supports_resume(agent));
            let extra = agents::resume::resume_session_args(agent, sid);
            assert_eq!(extra, vec!["--resume", sid]);

            let base_cmd = "claude --print";
            let cmd = format!("{} {}", base_cmd, extra.join(" "));
            assert_eq!(cmd, format!("claude --print --resume {}", sid));
        }
    }

    #[test]
    fn test_resume_decision_fresh_open_generates_new_session_id() {
        // Scenario: resume=false, agent=claude (supports resume)
        // Expected: new session ID generated with --session-id args

        let agent = "claude";
        let resume = false;
        let is_bare_shell = false;

        if !resume && !is_bare_shell && agents::resume::supports_resume(agent) {
            let sid = agents::resume::generate_session_id();
            assert!(!sid.is_empty());
            assert!(uuid::Uuid::parse_str(&sid).is_ok());

            let extra = agents::resume::create_session_args(agent, &sid);
            assert_eq!(extra.len(), 2);
            assert_eq!(extra[0], "--session-id");
            assert_eq!(extra[1], sid);
        } else {
            panic!("Should enter fresh-open-with-session-id branch");
        }
    }

    #[test]
    fn test_resume_decision_bare_shell_skips_all() {
        // Scenario: resume=true, is_bare_shell=true
        // Expected: resume logic is entirely skipped, no session ID changes
        let resume = true;
        let is_bare_shell = true;

        // The condition `resume && !is_bare_shell` should be false
        assert!(
            !(resume && !is_bare_shell),
            "bare shell should skip resume logic"
        );
        // And bare shell doesn't support resume either
        assert!(
            !((!is_bare_shell) && crate::agents::resume::supports_resume("claude")),
            "bare shell should skip session ID generation"
        );
    }

    #[test]
    fn test_resume_args_generated_correctly_for_claude() {
        let sid = "550e8400-e29b-41d4-a716-446655440000";
        let args = agents::resume::resume_session_args("claude", sid);
        assert_eq!(args, vec!["--resume", sid]);

        // Non-Claude should get nothing
        let args = agents::resume::resume_session_args("kiro", sid);
        assert!(args.is_empty());
    }

    #[test]
    fn test_session_id_survives_stop_lifecycle() {
        // Verify agent_session_id persists across stop (clear_agents + save + load)
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_sid_lifecycle_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        let session_id = "550e8400-e29b-41d4-a716-446655440000".to_string();
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test-project_sid-lifecycle_0".to_string(),
            Some(12345),
            Some("claude".to_string()),
            Some(1234567890),
            None,
            None,
            "claude --session-id 550e8400-e29b-41d4-a716-446655440000".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None,
        )
        .unwrap();

        let session = Session::new(
            "test-project_sid-lifecycle".into(),
            "test-project".into(),
            "sid-lifecycle".into(),
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
            Some(session_id.clone()),
            None,
            None,
        );

        // Save initial session
        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Simulate stop: clear agents, set stopped, save
        let mut stopped = persistence::find_session_by_name(&sessions_dir, "sid-lifecycle")
            .expect("Failed to find")
            .expect("Session should exist");
        stopped.clear_agents();
        stopped.status = SessionStatus::Stopped;
        persistence::save_session_to_file(&stopped, &sessions_dir)
            .expect("Failed to save stopped session");

        // Reload and verify session ID survived
        let reloaded = persistence::find_session_by_name(&sessions_dir, "sid-lifecycle")
            .expect("Failed to find")
            .expect("Session should exist");
        assert_eq!(reloaded.status, SessionStatus::Stopped);
        assert!(!reloaded.has_agents(), "Agents should be cleared");
        assert_eq!(
            reloaded.agent_session_id,
            Some(session_id),
            "agent_session_id must survive stop lifecycle"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // --- resolve_effective_runtime_mode tests ---

    #[test]
    fn test_resolve_runtime_mode_explicit_wins() {
        use crate::state::types::RuntimeMode;

        let config = crate::config::KildConfig::default();
        let (mode, source) = resolve_effective_runtime_mode(
            Some(RuntimeMode::Daemon),
            Some(RuntimeMode::Terminal),
            &config,
        );
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "explicit");
    }

    #[test]
    fn test_resolve_runtime_mode_session_when_no_explicit() {
        use crate::state::types::RuntimeMode;

        let config = crate::config::KildConfig::default();
        let (mode, source) =
            resolve_effective_runtime_mode(None, Some(RuntimeMode::Daemon), &config);
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "session");
    }

    #[test]
    fn test_resolve_runtime_mode_config_when_daemon_enabled() {
        use crate::state::types::RuntimeMode;

        let mut config = crate::config::KildConfig::default();
        config.daemon.enabled = Some(true);
        let (mode, source) = resolve_effective_runtime_mode(None, None, &config);
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "config");
    }

    #[test]
    fn test_resolve_runtime_mode_default_terminal() {
        use crate::state::types::RuntimeMode;

        let config = crate::config::KildConfig::default();
        let (mode, source) = resolve_effective_runtime_mode(None, None, &config);
        assert_eq!(mode, RuntimeMode::Terminal);
        assert_eq!(source, "default");
    }

    /// Validates the core of `open --all` behavior: when no explicit flag is passed,
    /// each session's stored runtime_mode is respected.
    #[test]
    fn test_resolve_runtime_mode_none_explicit_with_daemon_session() {
        use crate::state::types::RuntimeMode;

        let config = crate::config::KildConfig::default();
        // Simulates open --all (no flags): explicit=None, session has Daemon
        let (mode, source) =
            resolve_effective_runtime_mode(None, Some(RuntimeMode::Daemon), &config);
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "session");

        // Same with Terminal session
        let (mode, source) =
            resolve_effective_runtime_mode(None, Some(RuntimeMode::Terminal), &config);
        assert_eq!(mode, RuntimeMode::Terminal);
        assert_eq!(source, "session");
    }

    /// Regression test: explicit flags should override all sessions (open --all --daemon)
    #[test]
    fn test_resolve_runtime_mode_explicit_overrides_session_in_open_all() {
        use crate::state::types::RuntimeMode;

        let config = crate::config::KildConfig::default();
        // open --all --daemon: explicit=Daemon should override session=Terminal
        let (mode, source) = resolve_effective_runtime_mode(
            Some(RuntimeMode::Daemon),
            Some(RuntimeMode::Terminal),
            &config,
        );
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "explicit");

        // open --all --no-daemon: explicit=Terminal should override session=Daemon
        let (mode, source) = resolve_effective_runtime_mode(
            Some(RuntimeMode::Terminal),
            Some(RuntimeMode::Daemon),
            &config,
        );
        assert_eq!(mode, RuntimeMode::Terminal);
        assert_eq!(source, "explicit");
    }

    #[test]
    fn test_runtime_mode_persists_through_stop_reload_cycle() {
        use crate::state::types::RuntimeMode;
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_runtime_mode_persistence_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        let mut session = Session::new(
            "test-project_runtime-persist".into(),
            "test-project".into(),
            "runtime-persist".into(),
            worktree_dir,
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
            Some(RuntimeMode::Daemon),
        );

        // Simulate stop: clear agents, set stopped
        session.clear_agents();
        session.status = SessionStatus::Stopped;
        persistence::save_session_to_file(&session, &sessions_dir)
            .expect("Failed to save stopped session");

        // Reload from disk
        let reloaded = persistence::find_session_by_name(&sessions_dir, "runtime-persist")
            .expect("Failed to find")
            .expect("Session should exist");

        assert_eq!(reloaded.status, SessionStatus::Stopped);
        assert_eq!(
            reloaded.runtime_mode,
            Some(RuntimeMode::Daemon),
            "runtime_mode must survive stop + reload from disk"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // --- DefaultAgent "shell" fallback tests ---

    /// When session.agent is "shell" (created with --no-agent), DefaultAgent
    /// should fall back to the config default agent, not try to use "shell".
    #[test]
    fn test_default_agent_fallback_for_shell_sessions() {
        let config = crate::config::KildConfig::default();
        let session_agent = "shell";

        // Replicate the DefaultAgent branch logic from open_session
        let resolved = if session_agent == "shell" {
            config.agent.default.clone()
        } else {
            session_agent.to_string()
        };

        assert_eq!(
            resolved, "claude",
            "shell sessions should fall back to config default (claude)"
        );

        // The resolved agent must be a valid agent with a command
        assert!(
            config.get_agent_command(&resolved).is_ok(),
            "Config default agent must have a valid command"
        );
    }

    /// When session.agent is a real agent (e.g. "claude"), DefaultAgent
    /// should use the session's stored agent as before.
    #[test]
    fn test_default_agent_preserves_real_agent() {
        let session_agent = "claude";

        let resolved = if session_agent == "shell" {
            panic!("should not enter shell fallback");
        } else {
            session_agent.to_string()
        };

        assert_eq!(resolved, "claude");
    }

    /// Verify that "shell" is NOT a registered agent, confirming
    /// get_agent_command("shell") would fail without the fallback.
    #[test]
    fn test_shell_is_not_a_registered_agent() {
        assert!(
            !agents::is_valid_agent("shell"),
            "\"shell\" must not be a valid agent name"
        );
        assert!(
            agents::get_default_command("shell").is_none(),
            "\"shell\" must not have a default command"
        );

        let config = crate::config::KildConfig::default();
        assert!(
            config.get_agent_command("shell").is_err(),
            "get_agent_command(\"shell\") must return an error"
        );
    }

    // --- Yolo flag injection tests ---

    /// When yolo=true for a supported agent, yolo flags should be appended to the command.
    #[test]
    fn test_yolo_flag_injection_for_supported_agent() {
        let agent = "claude";
        let base_command = "claude";
        let yolo = true;
        let is_bare_shell = false;

        let result = if yolo && !is_bare_shell {
            if let Some(yolo_flags) = agents::get_yolo_flags(agent) {
                format!("{} {}", base_command, yolo_flags)
            } else {
                base_command.to_string()
            }
        } else {
            base_command.to_string()
        };

        assert_eq!(result, "claude --dangerously-skip-permissions");
    }

    /// When yolo=true for an unsupported agent, the command should be unchanged.
    #[test]
    fn test_yolo_flag_injection_for_unsupported_agent() {
        let agent = "opencode";
        let base_command = "opencode";
        let yolo = true;
        let is_bare_shell = false;

        let result = if yolo && !is_bare_shell {
            if let Some(yolo_flags) = agents::get_yolo_flags(agent) {
                format!("{} {}", base_command, yolo_flags)
            } else {
                base_command.to_string()
            }
        } else {
            base_command.to_string()
        };

        assert_eq!(result, "opencode");
    }

    /// When yolo=true but is_bare_shell=true, yolo injection is skipped entirely.
    #[test]
    fn test_yolo_flag_skipped_for_bare_shell() {
        let base_command = "/bin/zsh";
        let yolo = true;
        let is_bare_shell = true;

        let result = if yolo && !is_bare_shell {
            if let Some(yolo_flags) = agents::get_yolo_flags("claude") {
                format!("{} {}", base_command, yolo_flags)
            } else {
                base_command.to_string()
            }
        } else {
            base_command.to_string()
        };

        assert_eq!(result, "/bin/zsh");
    }

    /// When yolo=true and resume=true, both sets of flags should be present
    /// with yolo flags before resume args (matching open_session ordering).
    #[test]
    fn test_yolo_with_resume_flag_ordering() {
        let agent = "claude";
        let base_command = "claude";
        let session_id = "550e8400-e29b-41d4-a716-446655440000";

        // Step 1: Inject yolo flags (matches open_session step 3b)
        let yolo_flags = agents::get_yolo_flags(agent).unwrap();
        let after_yolo = format!("{} {}", base_command, yolo_flags);

        // Step 2: Append resume args (matches open_session step 4)
        let resume_args = agents::resume::resume_session_args(agent, session_id);
        let final_command = format!("{} {}", after_yolo, resume_args.join(" "));

        assert_eq!(
            final_command,
            format!(
                "claude --dangerously-skip-permissions --resume {}",
                session_id
            )
        );

        // Verify ordering: yolo flags come before resume args
        let yolo_pos = final_command
            .find("--dangerously-skip-permissions")
            .unwrap();
        let resume_pos = final_command.find("--resume").unwrap();
        assert!(
            yolo_pos < resume_pos,
            "Yolo flags must come before resume args"
        );
    }

    /// Verify yolo flags for all supported agents produce valid-looking flags.
    #[test]
    fn test_yolo_flags_format_validation() {
        for agent in ["claude", "amp", "kiro", "codex", "gemini"] {
            let flags = agents::get_yolo_flags(agent);
            assert!(flags.is_some(), "Agent '{}' should support yolo", agent);
            let flags = flags.unwrap();
            assert!(
                flags.starts_with("--"),
                "Yolo flags for '{}' should start with '--', got: {}",
                agent,
                flags
            );
            assert!(
                !flags.is_empty(),
                "Yolo flags for '{}' should not be empty",
                agent
            );
        }
    }

    /// DefaultAgent fallback should work with custom config defaults too,
    /// not just the hardcoded "claude".
    #[test]
    fn test_default_agent_fallback_uses_config_not_hardcoded() {
        let mut config = crate::config::KildConfig::default();
        config.agent.default = "gemini".to_string();

        let session_agent = "shell";

        let resolved = if session_agent == "shell" {
            config.agent.default.clone()
        } else {
            session_agent.to_string()
        };

        assert_eq!(
            resolved, "gemini",
            "shell fallback must use the config's default, not a hardcoded value"
        );
    }
}
