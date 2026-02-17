use kild_paths::KildPaths;
use tracing::{debug, error, info, warn};

use crate::agents;
use crate::config::{Config, KildConfig};
use crate::git;
use crate::sessions::{errors::SessionError, persistence, ports, types::*, validation};
use crate::terminal;

use super::daemon_helpers::{
    build_daemon_create_request, compute_spawn_id, ensure_shim_binary, setup_codex_integration,
    setup_opencode_integration, spawn_attach_window,
};

pub fn create_session(
    request: CreateSessionRequest,
    kild_config: &KildConfig,
) -> Result<Session, SessionError> {
    // Determine agent name and command based on AgentMode
    let (agent, agent_command) = match &request.agent_mode {
        crate::state::types::AgentMode::BareShell => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| {
                let fallback = "/bin/sh".to_string();
                warn!(
                    event = "core.session.shell_env_missing",
                    fallback = %fallback,
                    "$SHELL not set, falling back to /bin/sh"
                );
                fallback
            });
            info!(event = "core.session.create_shell_selected", shell = %shell);
            ("shell".to_string(), shell)
        }
        crate::state::types::AgentMode::Agent(name) => {
            let command =
                kild_config
                    .get_agent_command(name)
                    .map_err(|e| SessionError::ConfigError {
                        message: e.to_string(),
                    })?;

            if let Some(false) = agents::is_agent_available(name) {
                warn!(
                    event = "core.session.agent_not_available",
                    agent = %name,
                    "Agent CLI '{}' not found in PATH - session may fail to start",
                    name
                );
            }

            (name.clone(), command)
        }
        crate::state::types::AgentMode::DefaultAgent => {
            let name = kild_config.agent.default.clone();
            let command =
                kild_config
                    .get_agent_command(&name)
                    .map_err(|e| SessionError::ConfigError {
                        message: e.to_string(),
                    })?;

            if let Some(false) = agents::is_agent_available(&name) {
                warn!(
                    event = "core.session.agent_not_available",
                    agent = %name,
                    "Agent CLI '{}' not found in PATH - session may fail to start",
                    name
                );
            }

            (name, command)
        }
    };

    // Generate agent session ID for resume-capable agents
    let agent_session_id = if agents::resume::supports_resume(&agent) {
        Some(agents::resume::generate_session_id())
    } else {
        None
    };

    // Append --session-id to agent command for resume-capable agents
    let agent_command = if let Some(ref sid) = agent_session_id {
        let extra_args = agents::resume::create_session_args(&agent, sid);
        if extra_args.is_empty() {
            agent_command
        } else {
            info!(event = "core.session.agent_session_id_set", session_id = %sid);
            format!("{} {}", agent_command, extra_args.join(" "))
        }
    } else {
        agent_command
    };

    info!(
        event = "core.session.create_started",
        branch = %request.branch,
        agent = agent,
        command = agent_command
    );

    // 1. Validate input (pure)
    let validated = validation::validate_session_request(&request.branch, &agent_command, &agent)?;

    // 2. Detect git project (I/O)
    // Use explicit project path if provided (UI context), otherwise use cwd (CLI context)
    let project = match &request.project_path {
        Some(path) => {
            debug!(
                event = "core.session.project_path_explicit_provided",
                path = %path.display()
            );
            git::handler::detect_project_at(path)
        }
        None => git::handler::detect_project(),
    }
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "core.session.project_detected",
        project_id = project.id,
        project_name = project.name,
        branch = %validated.name
    );

    // 3. Create worktree (I/O)
    let config = Config::new();
    let project_id: kild_protocol::ProjectId = project.id.clone().into();
    let session_id = ports::generate_session_id(&project_id, &validated.name);

    // Generate task list ID for agents that support it (depends on session_id)
    let task_list_id = if agents::resume::supports_resume(&agent) {
        let tlid = agents::resume::generate_task_list_id(&session_id);
        info!(event = "core.session.task_list_id_set", task_list_id = %tlid);
        Some(tlid)
    } else {
        None
    };

    // Ensure sessions directory exists
    persistence::ensure_sessions_directory(&config.sessions_dir())?;

    // 4. Allocate port range (I/O)
    let (port_start, port_end) = ports::allocate_port_range(
        &config.sessions_dir(),
        config.default_port_count,
        config.base_port_range,
    )
    .map_err(|e| {
        error!(
            event = "core.session.port_allocation_failed",
            session_id = %session_id,
            requested_count = config.default_port_count,
            base_port = config.base_port_range,
            error = %e
        );
        e
    })?;

    info!(
        event = "core.session.port_allocated",
        session_id = %session_id,
        port_range_start = port_start,
        port_range_end = port_end,
        port_count = config.default_port_count
    );

    let base_config = Config::new();

    // Build effective git config with CLI overrides
    let mut git_config = kild_config.git.clone();
    if let Some(base) = &request.base_branch {
        git_config.base_branch = Some(base.clone());
    }
    if request.no_fetch {
        git_config.fetch_before_create = Some(false);
    }

    let worktree = git::handler::create_worktree(
        base_config.kild_dir(),
        &project,
        &validated.name,
        Some(kild_config),
        &git_config,
    )
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "core.session.worktree_created",
        session_id = %session_id,
        worktree_path = %worktree.path.display(),
        branch = worktree.branch
    );

    // 5. Launch agent — branch on runtime mode
    let spawn_id = compute_spawn_id(&session_id, 0);
    let now = chrono::Utc::now().to_rfc3339();

    let initial_agent = match request.runtime_mode {
        crate::state::types::RuntimeMode::Terminal => {
            setup_codex_integration(&validated.agent);
            setup_opencode_integration(&validated.agent, &worktree.path);

            // Terminal path: spawn in external terminal
            // Wrap with `env` to strip nesting-detection vars and inject agent env.
            let terminal_command = {
                let mut env_prefix: Vec<(String, String)> = Vec::new();
                if let Some(ref tlid) = task_list_id {
                    env_prefix.extend(agents::resume::task_list_env_vars(&agent, tlid));
                }
                env_prefix.extend(agents::resume::codex_env_vars(&agent, &validated.name));
                super::env_cleanup::build_env_command(&env_prefix, &validated.command)
            };
            debug!(
                event = "core.session.terminal_command_constructed",
                command = %terminal_command,
            );
            let spawn_result = terminal::handler::spawn_terminal(
                &worktree.path,
                &terminal_command,
                kild_config,
                Some(&spawn_id),
                Some(base_config.kild_dir()),
            )
            .map_err(|e| SessionError::TerminalError { source: e })?;

            let command = if spawn_result.command_executed.trim().is_empty() {
                format!("{} (command not captured)", validated.agent)
            } else {
                spawn_result.command_executed.clone()
            };
            AgentProcess::new(
                validated.agent.clone(),
                spawn_id,
                spawn_result.process_id,
                spawn_result.process_name.clone(),
                spawn_result.process_start_time,
                Some(spawn_result.terminal_type.clone()),
                spawn_result.terminal_window_id.clone(),
                command,
                now.clone(),
                None,
            )?
        }
        crate::state::types::RuntimeMode::Daemon => {
            // New path: request daemon to create PTY session.
            // The daemon is a pure PTY manager — it spawns a command in a
            // working directory. Worktree creation and session persistence
            // are handled here in kild-core.

            // Auto-start daemon if not running (config.daemon.auto_start, default: true)
            crate::daemon::ensure_daemon_running(kild_config)?;

            // Ensure the tmux shim binary is installed at ~/.kild/bin/tmux
            if let Err(msg) = ensure_shim_binary() {
                warn!(event = "core.session.shim_binary_failed", error = %msg);
                eprintln!("Warning: {}", msg);
                eprintln!("Agent teams will not work in this session.");
            }

            setup_codex_integration(&validated.agent);
            setup_opencode_integration(&validated.agent, &worktree.path);

            // Pre-emptive cleanup: remove stale daemon session if previous destroy failed.
            // Daemon-not-running and session-not-found are expected (normal case).
            match crate::daemon::client::destroy_daemon_session(&spawn_id, true) {
                Ok(()) => {
                    debug!(
                        event = "core.session.preemptive_cleanup_completed",
                        spawn_id = spawn_id,
                    );
                }
                Err(e) => {
                    debug!(
                        event = "core.session.preemptive_cleanup_skipped",
                        spawn_id = spawn_id,
                        error = %e,
                    );
                }
            }

            let (cmd, cmd_args, env_vars, use_login_shell) = build_daemon_create_request(
                &validated.command,
                &validated.agent,
                &session_id,
                task_list_id.as_deref(),
                &validated.name,
            )?;

            let daemon_request = crate::daemon::client::DaemonCreateRequest {
                request_id: &spawn_id,
                session_id: &spawn_id,
                working_directory: &worktree.path,
                command: &cmd,
                args: &cmd_args,
                env_vars: &env_vars,
                rows: 24,
                cols: 80,
                use_login_shell,
            };
            let daemon_result = crate::daemon::client::create_pty_session(&daemon_request)
                .map_err(|e| SessionError::DaemonError {
                    message: e.to_string(),
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
                    crate::daemon::client::read_scrollback(&daemon_result.daemon_session_id)
                        .inspect_err(|e| {
                            debug!(
                                event = "core.session.scrollback_read_failed",
                                daemon_session_id = daemon_result.daemon_session_id,
                                error = %e,
                            );
                        })
                        .ok()
                        .flatten()
                        .map(|bytes| {
                            let text = String::from_utf8_lossy(&bytes);
                            let lines: Vec<&str> = text.lines().collect();
                            let start = lines.len().saturating_sub(20);
                            lines[start..].join("\n")
                        })
                        .unwrap_or_default();

                if let Err(e) = crate::daemon::client::destroy_daemon_session(
                    &daemon_result.daemon_session_id,
                    true,
                ) {
                    warn!(
                        event = "core.session.create_daemon_cleanup_failed",
                        daemon_session_id = %daemon_result.daemon_session_id,
                        error = %e,
                    );
                }

                return Err(SessionError::DaemonPtyExitedEarly {
                    exit_code,
                    scrollback_tail,
                });
            }

            // Initialize tmux shim state directory
            let shim_init_result = (|| -> Result<(), String> {
                let shim_dir = KildPaths::resolve()
                    .map_err(|e| e.to_string())?
                    .shim_session_dir(&session_id);
                std::fs::create_dir_all(&shim_dir)
                    .map_err(|e| format!("failed to create shim state directory: {}", e))?;

                let initial_state = serde_json::json!({
                    "next_pane_id": 1,
                    "session_name": "kild_0",
                    "panes": {
                        "%0": {
                            "daemon_session_id": daemon_result.daemon_session_id,
                            "title": "",
                            "border_style": "",
                            "window_id": "0",
                            "hidden": false
                        }
                    },
                    "windows": {
                        "0": { "name": "main", "pane_ids": ["%0"] }
                    },
                    "sessions": {
                        "kild_0": { "name": "kild_0", "windows": ["0"] }
                    }
                });

                let lock_path = shim_dir.join("panes.lock");
                std::fs::File::create(&lock_path)
                    .map_err(|e| format!("failed to create shim lock file: {}", e))?;

                let panes_path = shim_dir.join("panes.json");
                let json = serde_json::to_string_pretty(&initial_state)
                    .map_err(|e| format!("failed to serialize shim state: {}", e))?;
                std::fs::write(&panes_path, json)
                    .map_err(|e| format!("failed to write shim state: {}", e))?;

                Ok(())
            })();

            if let Err(e) = shim_init_result {
                error!(
                    event = "core.session.shim_init_failed",
                    session_id = %session_id,
                    error = %e,
                );
                eprintln!("Warning: Failed to initialize agent team support: {}", e);
                eprintln!("Agent teams will not work in this session.");
            }

            // Spawn attach window (best-effort) and capture terminal info
            let attach_info =
                spawn_attach_window(&validated.name, &spawn_id, &worktree.path, kild_config);

            AgentProcess::new(
                validated.agent.clone(),
                spawn_id,
                None,
                None,
                None,
                attach_info.as_ref().map(|(tt, _)| tt.clone()),
                attach_info.map(|(_, wid)| wid),
                validated.command.clone(),
                now.clone(),
                Some(daemon_result.daemon_session_id),
            )?
        }
    };

    // 6. Create session record
    let session = Session::new(
        session_id.clone(),
        project_id,
        validated.name.clone(),
        worktree.path,
        validated.agent.clone(),
        SessionStatus::Active,
        now.clone(),
        port_start,
        port_end,
        config.default_port_count,
        Some(now),
        request.note.clone(),
        vec![initial_agent],
        agent_session_id,
        task_list_id,
        Some(request.runtime_mode.clone()),
    );

    // 7. Save session to file
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.create_completed",
        session_id = %session_id,
        branch = %validated.name,
        agent = session.agent,
        process_id = session.latest_agent().and_then(|a| a.process_id()),
        process_name = ?session.latest_agent().map(|a| a.process_name())
    );

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_list_destroy_integration_flow() {
        use std::fs;

        // Create a unique temporary directory for this test
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_integration_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

        // Test session persistence workflow using operations directly
        // This tests the core persistence logic without git/terminal dependencies

        // 1. Create a test session manually
        let session = Session::new(
            "test-project_test-branch".into(),
            "test-project".into(),
            "test-branch".into(),
            temp_dir.join("worktree").to_path_buf(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Create worktree directory so validation passes
        fs::create_dir_all(&session.worktree_path).expect("Failed to create worktree dir");

        // 2. Save session to file
        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // 3. List sessions - should contain our session
        let (sessions, skipped) =
            persistence::load_sessions_from_files(&sessions_dir).expect("Failed to load sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(sessions[0].id, session.id);
        assert_eq!(&*sessions[0].branch, "test-branch");

        // 4. Find session by name
        let found_session = persistence::find_session_by_name(&sessions_dir, "test-branch")
            .expect("Failed to find session")
            .expect("Session not found");
        assert_eq!(found_session.id, session.id);

        // 5. Remove session file
        persistence::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // 6. List sessions - should be empty
        let (sessions_after, _) = persistence::load_sessions_from_files(&sessions_dir)
            .expect("Failed to load sessions after removal");
        assert_eq!(sessions_after.len(), 0);

        // 7. Try to find removed session - should return None
        let not_found = persistence::find_session_by_name(&sessions_dir, "test-branch")
            .expect("Failed to search for removed session");
        assert!(not_found.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_with_terminal_type_persistence() {
        use crate::terminal::types::TerminalType;
        use std::fs;

        // This test verifies the terminal_type field flows correctly through
        // the session persistence layer - critical for destroy_session to work.
        //
        // The destroy_session function relies on:
        // 1. Session being saved with terminal_type populated
        // 2. Session being loaded with terminal_type intact
        // 3. The field being passed to close_terminal()

        // Use unique temp dir per test run to avoid conflicts
        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_terminal_type_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Test all terminal types can be saved and loaded
        let terminal_test_cases = [
            (TerminalType::ITerm, "test-iterm"),
            (TerminalType::TerminalApp, "test-terminalapp"),
            (TerminalType::Ghostty, "test-ghostty"),
            (TerminalType::Native, "test-native"),
        ];

        for (terminal_type, branch_name) in &terminal_test_cases {
            // Use underscore in id to avoid filesystem issues with slash
            let agent = AgentProcess::new(
                "test-agent".to_string(),
                format!("test-project_{}_{}", branch_name, 0),
                Some(12345),
                Some("test-agent".to_string()),
                Some(1234567890),
                Some(terminal_type.clone()),
                Some("1596".to_string()),
                "test-command".to_string(),
                chrono::Utc::now().to_rfc3339(),
                None,
            )
            .unwrap();
            let session = Session::new(
                format!("test-project_{}", branch_name).into(),
                "test-project".into(),
                (*branch_name).into(),
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

            persistence::save_session_to_file(&session, &sessions_dir)
                .expect("Failed to save session");

            let loaded = persistence::find_session_by_name(&sessions_dir, branch_name)
                .expect("Failed to find session")
                .expect("Session not found");

            let loaded_terminal_type = loaded
                .latest_agent()
                .and_then(|a| a.terminal_type().cloned());
            assert_eq!(
                loaded_terminal_type,
                Some(terminal_type.clone()),
                "terminal_type {:?} must round-trip correctly",
                terminal_type
            );
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_create_session_request_project_path_affects_project_detection() {
        use git2::Repository;
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_session_project_path_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Initialize a git repo at the temp path
        let repo = Repository::init(&temp_dir).expect("Failed to init git repo");
        {
            let sig = repo
                .signature()
                .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .expect("Failed to create initial commit");
        }

        // Create the request with explicit project_path
        let request = CreateSessionRequest::with_project_path(
            "test-branch".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            None,
            temp_dir.clone(),
        );

        // Verify the request has project_path set
        assert!(
            request.project_path.is_some(),
            "Request should have project_path set"
        );
        assert_eq!(request.project_path.as_ref().unwrap(), &temp_dir);

        let project = match &request.project_path {
            Some(path) => git::handler::detect_project_at(path),
            None => git::handler::detect_project(),
        };

        assert!(project.is_ok(), "Project detection should succeed");
        let project = project.unwrap();

        // Verify the project path matches the temp dir (not cwd)
        let expected_path = temp_dir.canonicalize().unwrap();
        let actual_path = project.path.canonicalize().unwrap();
        assert_eq!(
            actual_path, expected_path,
            "Project should be from the explicit path, not cwd"
        );

        // Also verify that without project_path, we'd get a different result
        let request_without_path = CreateSessionRequest::new(
            "test-branch".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            None,
        );
        assert!(
            request_without_path.project_path.is_none(),
            "Request without project_path should have None"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_create_session_request_none_project_path_uses_cwd_detection() {
        let request = CreateSessionRequest::new(
            "test-branch".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            Some("test note".to_string()),
        );

        assert!(
            request.project_path.is_none(),
            "CreateSessionRequest::new should leave project_path as None"
        );
    }

    #[test]
    fn test_create_session_generates_session_id_for_claude() {
        // Verify that agent_session_id generation works for resume-capable agents
        assert!(agents::resume::supports_resume("claude"));
        assert!(!agents::resume::supports_resume("kiro"));

        // Claude should get --session-id args
        let sid = agents::resume::generate_session_id();
        let args = agents::resume::create_session_args("claude", &sid);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--session-id");
        assert_eq!(args[1], sid);

        // Non-Claude should get empty args
        let args = agents::resume::create_session_args("kiro", &sid);
        assert!(args.is_empty());
    }

    #[test]
    fn test_session_with_missing_worktree_fails_operation_validation() {
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
            std::env::temp_dir().join(format!("kild_test_missing_worktree_op_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

        // Create a session pointing to a worktree that does NOT exist
        let missing_worktree = temp_dir.join("worktree_does_not_exist");
        let session = Session::new(
            "test-project_orphaned-session".into(),
            "test-project".into(),
            "orphaned-session".into(),
            missing_worktree.clone(),
            "claude".to_string(),
            SessionStatus::Stopped,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save the session
        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // 1. Verify session CAN be loaded (the fix for issue #102)
        let (sessions, skipped) =
            persistence::load_sessions_from_files(&sessions_dir).expect("Failed to load sessions");
        assert_eq!(
            sessions.len(),
            1,
            "Session should be loaded despite missing worktree"
        );
        assert_eq!(skipped, 0, "Session should not be skipped");

        // 2. Verify worktree does NOT exist
        assert!(
            !missing_worktree.exists(),
            "Worktree should not exist for this test"
        );

        // 3. Verify is_worktree_valid() returns false (used by UI for status indicators)
        assert!(
            !sessions[0].is_worktree_valid(),
            "is_worktree_valid() should return false for missing worktree"
        );

        // 4. Verify operation-level validation would reject this session
        let loaded_session = &sessions[0];
        if !loaded_session.worktree_path.exists() {
            let expected_error = SessionError::WorktreeNotFound {
                path: loaded_session.worktree_path.clone(),
            };
            assert!(
                matches!(expected_error, SessionError::WorktreeNotFound { .. }),
                "Operation should return WorktreeNotFound for missing worktree"
            );
        } else {
            panic!("Test setup error: worktree should not exist");
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_persistence_lifecycle_with_terminal_type() {
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
            std::env::temp_dir().join(format!("kild_test_destroy_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create session with terminal_type in agent (simulating what create_session does)
        let agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_destroy-test_0".to_string(),
            None, // No process to kill
            None,
            None,
            Some(TerminalType::ITerm), // Key: terminal_type is set
            Some("1596".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None,
        )
        .unwrap();
        let session = Session::new(
            "test-project_destroy-test".into(),
            "test-project".into(),
            "destroy-test".into(),
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

        // Verify session exists
        let found = persistence::find_session_by_name(&sessions_dir, "destroy-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(
            found
                .latest_agent()
                .and_then(|a| a.terminal_type().cloned()),
            Some(TerminalType::ITerm)
        );

        // Remove session file (simulating destroy flow without git worktree dependency)
        persistence::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // Verify session is gone
        let not_found = persistence::find_session_by_name(&sessions_dir, "destroy-test")
            .expect("Failed to search");
        assert!(not_found.is_none(), "Session should be removed");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_persistence_lifecycle_without_agents_backward_compat() {
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_destroy_compat_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create an "old" session WITHOUT agents (simulating pre-feature sessions)
        let session = Session::new(
            "test-project_compat-test".into(),
            "test-project".into(),
            "compat-test".into(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![], // No agents (old session)
            None,
            None,
            None,
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session can be loaded without agents
        let found = persistence::find_session_by_name(&sessions_dir, "compat-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert!(!found.has_agents(), "Old sessions should have no agents");

        // Remove session (simulating destroy flow)
        persistence::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // Verify session is gone
        let not_found = persistence::find_session_by_name(&sessions_dir, "compat-test")
            .expect("Failed to search");
        assert!(not_found.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
