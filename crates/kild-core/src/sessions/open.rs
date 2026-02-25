use tracing::{error, info, warn};

use crate::agents;
use crate::sessions::{errors::SessionError, persistence, types::*};
use kild_config::{Config, KildConfig};
use kild_protocol::{OpenMode, RuntimeMode};

use super::daemon_helpers::{
    AgentSpawnParams, compute_spawn_id, deliver_initial_prompt, spawn_and_save_attach_window,
    spawn_daemon_agent, spawn_terminal_agent,
};
use super::dropbox;

/// Resolve the effective runtime mode for `open_session`.
///
/// Priority: explicit CLI flag > session's stored mode > config > Terminal default.
/// Returns the resolved mode and its source label for logging.
fn resolve_effective_runtime_mode(
    explicit: Option<RuntimeMode>,
    from_session: Option<RuntimeMode>,
    config: &kild_config::KildConfig,
) -> (RuntimeMode, &'static str) {
    if let Some(mode) = explicit {
        return (mode, "explicit");
    }
    if let Some(mode) = from_session {
        return (mode, "session");
    }
    if config.is_daemon_enabled() {
        (RuntimeMode::Daemon, "config")
    } else {
        (RuntimeMode::Terminal, "default")
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
    mode: kild_protocol::OpenMode,
    runtime_mode: Option<RuntimeMode>,
    resume: bool,
    yolo: bool,
    no_attach: bool,
    initial_prompt: Option<&str>,
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
    let is_bare_shell = matches!(mode, OpenMode::BareShell);
    let (agent, agent_command) =
        match mode {
            OpenMode::BareShell => {
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
            OpenMode::Agent(name) => {
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
            OpenMode::DefaultAgent => {
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

    let use_daemon = effective_runtime_mode == RuntimeMode::Daemon;

    let spawn_params = AgentSpawnParams {
        branch: &session.branch,
        agent: &agent,
        agent_command: &agent_command,
        worktree_path: &session.worktree_path,
        session_id: &session.id,
        spawn_id: &spawn_id,
        task_list_id: new_task_list_id.as_deref(),
        project_id: &session.project_id,
        kild_config: &kild_config,
    };

    let new_agent = if use_daemon {
        let agent_process = spawn_daemon_agent(&spawn_params)?;

        // Open-only: deliver initial prompt after the TUI has settled (best-effort).
        if let Some(prompt) = initial_prompt
            && let Some(dsid) = agent_process.daemon_session_id()
        {
            let delivered = deliver_initial_prompt(dsid, prompt);
            // Clear .idle_sent gate so the next idle event notifies the brain.
            // deliver_initial_prompt bypasses dropbox::write_task(), which normally
            // handles this. Only clear when delivery succeeded — clearing on failure
            // would cause a phantom brain notification on the next idle event.
            if delivered {
                dropbox::clear_idle_gate(&session.project_id, &session.branch);
            }
        }

        agent_process
    } else {
        spawn_terminal_agent(&spawn_params)?
    };

    let now = chrono::Utc::now().to_rfc3339();
    session.status = SessionStatus::Active;
    session.last_activity = Some(now);
    session.add_agent(new_agent);

    // Update agent session ID for resume support.
    // Preserve the previous ID in history so the original conversation remains recoverable.
    if let Some(sid) = new_agent_session_id
        && session.rotate_agent_session_id(sid.clone())
    {
        warn!(
            event = "core.session.agent_session_id_rotated",
            branch = name,
            new_id = %sid,
            "Previous agent session ID moved to history — use --resume to continue an existing conversation"
        );
    }

    // Update task list ID for task list persistence
    if let Some(tlid) = new_task_list_id {
        session.task_list_id = Some(tlid);
    }

    // Update runtime mode so future opens auto-detect correctly
    let is_daemon = effective_runtime_mode == RuntimeMode::Daemon;
    session.runtime_mode = Some(effective_runtime_mode);

    // 6. Save session BEFORE spawning attach window so `kild attach` can find it
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    // 7. Spawn attach window (best-effort) and update session with terminal info.
    // Skipped when no_attach is set — for programmatic opens (e.g. brain reopening workers)
    // where a Ghostty window popping up is undesirable.
    if is_daemon && !no_attach {
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
            OpenMode::DefaultAgent,
            Some(RuntimeMode::Terminal),
            false,
            false,
            false,
            None,
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
        let config = kild_config::KildConfig::default();
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
        let config = kild_config::KildConfig::default();
        let (mode, source) =
            resolve_effective_runtime_mode(None, Some(RuntimeMode::Daemon), &config);
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "session");
    }

    #[test]
    fn test_resolve_runtime_mode_config_when_daemon_enabled() {
        let mut config = kild_config::KildConfig::default();
        config.daemon.enabled = Some(true);
        let (mode, source) = resolve_effective_runtime_mode(None, None, &config);
        assert_eq!(mode, RuntimeMode::Daemon);
        assert_eq!(source, "config");
    }

    #[test]
    fn test_resolve_runtime_mode_default_terminal() {
        let config = kild_config::KildConfig::default();
        let (mode, source) = resolve_effective_runtime_mode(None, None, &config);
        assert_eq!(mode, RuntimeMode::Terminal);
        assert_eq!(source, "default");
    }

    /// Validates the core of `open --all` behavior: when no explicit flag is passed,
    /// each session's stored runtime_mode is respected.
    #[test]
    fn test_resolve_runtime_mode_none_explicit_with_daemon_session() {
        let config = kild_config::KildConfig::default();
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
        let config = kild_config::KildConfig::default();
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
        use RuntimeMode;
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
        let config = kild_config::KildConfig::default();
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

        let config = kild_config::KildConfig::default();
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
        let mut config = kild_config::KildConfig::default();
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

    // --- agent_session_id_history tests (Bug #572) ---

    /// Fresh open on a session with an existing agent_session_id should
    /// preserve the old ID in history before overwriting.
    #[test]
    fn test_fresh_open_preserves_previous_session_id_in_history() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_sid_history_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("create worktree dir");

        let original_sid = "aaaa0000-0000-0000-0000-000000000001".to_string();
        let new_sid = "bbbb0000-0000-0000-0000-000000000002".to_string();

        let mut session = Session::new(
            "test-project_sid-history".into(),
            "test-project".into(),
            "sid-history".into(),
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
            Some(original_sid.clone()),
            None,
            None,
        );

        assert!(session.rotate_agent_session_id(new_sid.clone()));

        // Verify: new ID is active, old ID is in history
        assert_eq!(session.agent_session_id, Some(new_sid));
        assert_eq!(session.agent_session_id_history, vec![original_sid.clone()]);

        // Verify history survives serialization round-trip
        persistence::save_session_to_file(&session, &sessions_dir).expect("save");
        let reloaded = persistence::find_session_by_name(&sessions_dir, "sid-history")
            .expect("find")
            .expect("exists");
        assert_eq!(
            reloaded.agent_session_id_history,
            vec![original_sid],
            "agent_session_id_history must survive save/load"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    /// Resume (same ID) should NOT add a duplicate to history.
    #[test]
    fn test_resume_does_not_duplicate_session_id_in_history() {
        let sid = "cccc0000-0000-0000-0000-000000000003".to_string();
        let mut session = Session::new_for_test(
            "no-dup",
            std::env::temp_dir().join("kild_test_no_dup_worktree"),
        );
        session.agent_session_id = Some(sid.clone());

        assert!(!session.rotate_agent_session_id(sid));
        assert!(
            session.agent_session_id_history.is_empty(),
            "Resume with same ID must not add to history"
        );
    }

    /// Multiple fresh opens should accumulate all previous IDs in order.
    #[test]
    fn test_multiple_fresh_opens_accumulate_history() {
        let ids: Vec<String> = (1..=4)
            .map(|i| format!("dddd0000-0000-0000-0000-00000000000{i}"))
            .collect();

        let mut session = Session::new_for_test(
            "multi-open",
            std::env::temp_dir().join("kild_test_multi_open_worktree"),
        );
        session.agent_session_id = Some(ids[0].clone());

        for new_sid in &ids[1..] {
            session.rotate_agent_session_id(new_sid.clone());
        }

        assert_eq!(session.agent_session_id, Some(ids[3].clone()));
        assert_eq!(session.agent_session_id_history, ids[..3]);
    }

    /// Empty history serializes cleanly (skip_serializing_if = "Vec::is_empty").
    #[test]
    fn test_empty_history_not_serialized() {
        let session = Session::new_for_test(
            "no-history",
            std::env::temp_dir().join("kild_test_no_history_worktree"),
        );
        let json = serde_json::to_string(&session).expect("serialize");
        assert!(
            !json.contains("agent_session_id_history"),
            "Empty history should not appear in JSON"
        );
    }

    /// Legacy session files without `agent_session_id_history` must deserialize
    /// cleanly with an empty vec (backward compatibility via #[serde(default)]).
    #[test]
    fn test_legacy_session_without_history_deserializes_cleanly() {
        let legacy_json = r#"{
            "id": "test-proj_my-branch",
            "project_id": "test-proj",
            "branch": "my-branch",
            "worktree_path": "/tmp/worktree",
            "agent": "claude",
            "status": "stopped",
            "created_at": "2025-01-01T00:00:00Z",
            "agent_session_id": "aaaa-0000"
        }"#;
        let session: Session =
            serde_json::from_str(legacy_json).expect("legacy format must deserialize");
        assert!(
            session.agent_session_id_history.is_empty(),
            "Legacy sessions without the field must deserialize with empty history"
        );
        assert_eq!(session.agent_session_id.as_deref(), Some("aaaa-0000"));
    }
}
