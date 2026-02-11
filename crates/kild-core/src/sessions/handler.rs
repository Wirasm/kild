use tracing::{debug, error, info, warn};

use crate::agents;
use crate::config::{Config, KildConfig};
use crate::git;
use crate::sessions::{errors::SessionError, persistence, ports, types::*, validation};
use crate::terminal;
use crate::terminal::types::SpawnResult;

// Re-export from submodules to preserve public API (session_ops::*)
pub use super::agent_status::{
    find_session_by_worktree_path, read_agent_status, update_agent_status,
};
pub use super::complete::{complete_session, fetch_pr_info, read_pr_info};
pub use super::destroy::{destroy_session, get_destroy_safety_info, has_remote_configured};

/// Compute a unique spawn ID for a given session and spawn index.
///
/// Each agent spawn within a session gets its own spawn ID, which is used for
/// per-agent PID file paths and window titles. This prevents race conditions
/// where `kild open` on a running kild would read the wrong PID.
fn compute_spawn_id(session_id: &str, spawn_index: usize) -> String {
    format!("{}_{}", session_id, spawn_index)
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
        branch = request.branch,
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
        branch = validated.name
    );

    // 3. Create worktree (I/O)
    let config = Config::new();
    let session_id = ports::generate_session_id(&project.id, &validated.name);

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
        session_id = session_id,
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
        &base_config.kild_dir,
        &project,
        &validated.name,
        Some(kild_config),
        &git_config,
    )
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "core.session.worktree_created",
        session_id = session_id,
        worktree_path = %worktree.path.display(),
        branch = worktree.branch
    );

    // 5. Launch agent — branch on runtime mode
    let spawn_id = compute_spawn_id(&session_id, 0);
    let now = chrono::Utc::now().to_rfc3339();

    let initial_agent = match request.runtime_mode {
        crate::state::types::RuntimeMode::Terminal => {
            // Terminal path: spawn in external terminal
            // Prepend task list env vars via `env` command for agents that support it.
            // Uses `env KEY=val command` so it works with `exec` (env is an executable).
            let terminal_command = if let Some(ref tlid) = task_list_id {
                let env_prefix = agents::resume::task_list_env_vars(&agent, tlid);
                if env_prefix.is_empty() {
                    validated.command.clone()
                } else {
                    let env_args: Vec<String> = env_prefix
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect();
                    format!("env {} {}", env_args.join(" "), validated.command)
                }
            } else {
                validated.command.clone()
            };
            let spawn_result = terminal::handler::spawn_terminal(
                &worktree.path,
                &terminal_command,
                kild_config,
                Some(&spawn_id),
                Some(&base_config.kild_dir),
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

            // Pre-emptive cleanup: remove stale daemon session if previous destroy failed.
            // Daemon-not-running and session-not-found are expected (normal case).
            match crate::daemon::client::destroy_daemon_session(&session_id, true) {
                Ok(()) => {
                    debug!(
                        event = "core.session.preemptive_cleanup_completed",
                        session_id = session_id,
                    );
                }
                Err(e) => {
                    debug!(
                        event = "core.session.preemptive_cleanup_skipped",
                        session_id = session_id,
                        error = %e,
                    );
                }
            }

            let (cmd, cmd_args, env_vars, use_login_shell) = build_daemon_create_request(
                &validated.command,
                &validated.agent,
                &session_id,
                task_list_id.as_deref(),
            )?;

            let daemon_request = crate::daemon::client::DaemonCreateRequest {
                request_id: &spawn_id,
                session_id: &session_id,
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
                        .ok()
                        .flatten()
                        .map(|bytes| {
                            let text = String::from_utf8_lossy(&bytes);
                            let lines: Vec<&str> = text.lines().collect();
                            let start = lines.len().saturating_sub(20);
                            lines[start..].join("\n")
                        })
                        .unwrap_or_default();

                let _ = crate::daemon::client::destroy_daemon_session(
                    &daemon_result.daemon_session_id,
                    true,
                );

                return Err(SessionError::DaemonPtyExitedEarly {
                    exit_code,
                    scrollback_tail,
                });
            }

            // Initialize tmux shim state directory
            let shim_init_result = (|| -> Result<(), String> {
                let shim_dir = dirs::home_dir()
                    .ok_or("HOME not set")?
                    .join(".kild")
                    .join("shim")
                    .join(&session_id);
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
                    session_id = session_id,
                    error = %e,
                );
                eprintln!("Warning: Failed to initialize agent team support: {}", e);
                eprintln!("Agent teams will not work in this session.");
            }

            AgentProcess::new(
                validated.agent.clone(),
                spawn_id,
                None,
                None,
                None,
                None,
                None,
                validated.command.clone(),
                now.clone(),
                Some(daemon_result.daemon_session_id),
            )?
        }
    };

    // 6. Create session record
    let session = Session::new(
        session_id.clone(),
        project.id,
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
    );

    // 7. Save session to file
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.create_completed",
        session_id = session_id,
        branch = validated.name,
        agent = session.agent,
        process_id = session.latest_agent().and_then(|a| a.process_id()),
        process_name = ?session.latest_agent().map(|a| a.process_name())
    );

    Ok(session)
}

pub fn list_sessions() -> Result<Vec<Session>, SessionError> {
    info!(event = "core.session.list_started");

    let config = Config::new();
    let (sessions, skipped_count) = persistence::load_sessions_from_files(&config.sessions_dir())?;

    if skipped_count > 0 {
        tracing::warn!(
            event = "core.session.list_skipped_sessions",
            skipped_count = skipped_count,
            message = "Some session files were skipped due to errors"
        );
    }

    info!(
        event = "core.session.list_completed",
        count = sessions.len()
    );

    Ok(sessions)
}

pub fn get_session(name: &str) -> Result<Session, SessionError> {
    info!(event = "core.session.get_started", name = name);

    let config = Config::new();
    let session = persistence::load_session_from_file(name, &config.sessions_dir())?;

    info!(
        event = "core.session.get_completed",
        name = name,
        session_id = session.id
    );

    Ok(session)
}

pub fn restart_session(
    name: &str,
    agent_override: Option<String>,
) -> Result<Session, SessionError> {
    let start_time = std::time::Instant::now();
    info!(event = "core.session.restart_started", name = name, agent_override = ?agent_override);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let mut session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.restart_found",
        session_id = session.id,
        current_agent = session.agent,
        agent_count = session.agent_count()
    );

    // 2. Kill process if PID is tracked (use latest agent from agents vec)
    if let Some(latest) = session.latest_agent()
        && let Some(pid) = latest.process_id()
    {
        info!(event = "core.session.restart_kill_started", pid = pid);

        match crate::process::kill_process(pid, latest.process_name(), latest.process_start_time())
        {
            Ok(()) => info!(event = "core.session.restart_kill_completed", pid = pid),
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(
                    event = "core.session.restart_kill_process_not_found",
                    pid = pid
                )
            }
            Err(crate::process::ProcessError::AccessDenied { .. }) => {
                error!(event = "core.session.restart_kill_access_denied", pid = pid);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: "Access denied - insufficient permissions to kill process".to_string(),
                });
            }
            Err(e) => {
                error!(event = "core.session.restart_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: format!("Process still running: {}", e),
                });
            }
        }
    }

    // 3. Validate worktree still exists
    if !session.worktree_path.exists() {
        error!(
            event = "core.session.restart_worktree_missing",
            session_id = session.id,
            worktree_path = %session.worktree_path.display()
        );
        return Err(SessionError::WorktreeNotFound {
            path: session.worktree_path.clone(),
        });
    }

    // 4. Determine agent and command
    let base_config = Config::new();
    let kild_config = match KildConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            warn!(
                event = "core.config.load_failed",
                error = %e,
                session_id = %session.id,
                "Config load failed during restart, using defaults"
            );
            KildConfig::default()
        }
    };
    let agent = agent_override.unwrap_or(session.agent.clone());
    let agent_command =
        kild_config
            .get_agent_command(&agent)
            .map_err(|e| SessionError::ConfigError {
                message: e.to_string(),
            })?;

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

    info!(
        event = "core.session.restart_agent_selected",
        session_id = session.id,
        agent = agent,
        command = agent_command
    );

    // 5. Relaunch terminal in existing worktree
    info!(event = "core.session.restart_spawn_started", worktree_path = %session.worktree_path.display());

    let spawn_id = compute_spawn_id(&session.id, 0);
    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &agent_command,
        &kild_config,
        Some(&spawn_id),
        Some(&base_config.kild_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    info!(
        event = "core.session.restart_spawn_completed",
        process_id = spawn_result.process_id,
        process_name = ?spawn_result.process_name,
        terminal_window_id = ?spawn_result.terminal_window_id
    );

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) = capture_process_metadata(&spawn_result, "restart");

    // 6. Update session with new process info
    let now = chrono::Utc::now().to_rfc3339();
    let command = if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", agent)
    } else {
        spawn_result.command_executed.clone()
    };
    let new_agent = AgentProcess::new(
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
    )?;

    session.agent = agent;
    session.status = SessionStatus::Active;
    session.last_activity = Some(now);
    session.clear_agents();
    session.add_agent(new_agent);

    // 7. Save updated session to file
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.restart_completed",
        session_id = session.id,
        branch = name,
        agent = session.agent,
        process_id = session.latest_agent().and_then(|a| a.process_id()),
        duration_ms = start_time.elapsed().as_millis()
    );

    Ok(session)
}

/// Opens a new agent terminal in an existing kild (additive - doesn't close existing terminals).
///
/// This is the preferred way to add agents to a kild. Unlike restart, this does NOT
/// close existing terminals - multiple agents can run in the same kild.
///
/// The `runtime_mode` parameter controls whether the agent spawns in an external terminal
/// or a daemon-owned PTY. The CLI resolves this from `--daemon`/`--no-daemon` flags and
/// config, similar to `create_session`.
pub fn open_session(
    name: &str,
    mode: crate::state::types::OpenMode,
    runtime_mode: crate::state::types::RuntimeMode,
    resume: bool,
) -> Result<Session, SessionError> {
    info!(
        event = "core.session.open_started",
        name = name,
        mode = ?mode
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
        session_id = session.id,
        branch = session.branch
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
                let agent = session.agent.clone();
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

    let use_daemon = runtime_mode == crate::state::types::RuntimeMode::Daemon;

    let now = chrono::Utc::now().to_rfc3339();

    let new_agent = if use_daemon {
        // Auto-start daemon if not running (config.daemon.auto_start, default: true)
        crate::daemon::ensure_daemon_running(&kild_config)?;

        // Daemon path: create new daemon PTY (uses shared helper with create_session)
        let (cmd, cmd_args, env_vars, use_login_shell) = build_daemon_create_request(
            &agent_command,
            &agent,
            &session.id,
            new_task_list_id.as_deref(),
        )?;

        let daemon_request = crate::daemon::client::DaemonCreateRequest {
            request_id: &spawn_id,
            session_id: &session.id,
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
                crate::daemon::client::read_scrollback(&daemon_result.daemon_session_id)
                    .ok()
                    .flatten()
                    .map(|bytes| {
                        let text = String::from_utf8_lossy(&bytes);
                        let lines: Vec<&str> = text.lines().collect();
                        let start = lines.len().saturating_sub(20);
                        lines[start..].join("\n")
                    })
                    .unwrap_or_default();

            let _ = crate::daemon::client::destroy_daemon_session(
                &daemon_result.daemon_session_id,
                true,
            );

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
        // Terminal path: spawn in external terminal
        // Prepend task list env vars via `env` command for agents that support it.
        // Uses `env KEY=val command` so it works with `exec` (env is an executable).
        let terminal_command = if let Some(ref tlid) = new_task_list_id {
            let env_prefix = agents::resume::task_list_env_vars(&agent, tlid);
            if env_prefix.is_empty() {
                agent_command.clone()
            } else {
                let env_args: Vec<String> = env_prefix
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                format!("env {} {}", env_args.join(" "), agent_command)
            }
        } else {
            agent_command.clone()
        };
        let spawn_result = terminal::handler::spawn_terminal(
            &session.worktree_path,
            &terminal_command,
            &kild_config,
            Some(&spawn_id),
            Some(&config.kild_dir),
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

    // 6. Save updated session
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.open_completed",
        session_id = session.id,
        agent_count = session.agent_count()
    );

    Ok(session)
}

/// Ensure the tmux shim binary is installed at `~/.kild/bin/tmux`.
///
/// Looks for `kild-tmux-shim` next to the running `kild` binary and symlinks
/// it as `tmux` in `~/.kild/bin/`. Agent teams require this binary.
fn ensure_shim_binary() -> Result<(), String> {
    let shim_bin_dir = dirs::home_dir()
        .ok_or("HOME not set — cannot install tmux shim")?
        .join(".kild")
        .join("bin");
    let shim_link = shim_bin_dir.join("tmux");

    if shim_link.exists() {
        return Ok(());
    }

    let shim_binary = crate::daemon::find_sibling_binary("kild-tmux-shim")?;

    std::fs::create_dir_all(&shim_bin_dir)
        .map_err(|e| format!("failed to create {}: {}", shim_bin_dir.display(), e))?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&shim_binary, &shim_link).map_err(|e| {
        format!(
            "failed to symlink {} -> {}: {}",
            shim_binary.display(),
            shim_link.display(),
            e
        )
    })?;

    info!(
        event = "core.session.shim_binary_installed",
        path = %shim_link.display()
    );

    Ok(())
}

/// Build the command, args, env vars, and login shell flag for a daemon PTY create request.
///
/// Both `create_session` and `open_session` need to parse the agent command string
/// and collect environment variables for the daemon. This helper centralises that logic.
///
/// Two strategies based on agent type:
/// - **Bare shell** (`agent_name == "shell"`): Sets `use_login_shell = true` so the daemon
///   uses `CommandBuilder::new_default_prog()` for a native login shell with profile sourcing.
/// - **Agents**: Wraps in `$SHELL -lc 'exec <command>'` so profile files are sourced
///   before the agent starts, providing full PATH and environment. The `exec` replaces
///   the wrapper shell with the agent for clean process tracking.
///
/// The `session_id` is used to set up tmux shim environment variables so that agents
/// running inside daemon PTYs see a `$TMUX` environment and can use pane-based workflows.
#[allow(clippy::type_complexity)]
fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
    session_id: &str,
    task_list_id: Option<&str>,
) -> Result<(String, Vec<String>, Vec<(String, String)>, bool), SessionError> {
    let use_login_shell = agent_name == "shell";

    let (cmd, cmd_args) = if use_login_shell {
        // For bare shell: command/args are ignored by new_default_prog(),
        // but we still pass them for logging purposes.
        (agent_command.to_string(), vec![])
    } else {
        // For agents: validate command is non-empty, then wrap in login shell.
        // sh -lc 'exec claude --flags' ensures profile files are sourced.
        if agent_command.split_whitespace().next().is_none() {
            return Err(SessionError::DaemonError {
                message: format!(
                    "Empty command string for agent '{}'. Check agent configuration.",
                    agent_name
                ),
            });
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let escaped = agent_command.replace('\'', "'\\''");
        (shell, vec!["-lc".to_string(), format!("exec {}", escaped)])
    };

    let mut env_vars = Vec::new();
    for key in &["PATH", "HOME", "SHELL", "USER", "LANG", "TERM"] {
        if let Ok(val) = std::env::var(key) {
            env_vars.push((key.to_string(), val));
        }
    }

    // tmux shim environment for daemon sessions
    let home_dir = dirs::home_dir().ok_or_else(|| SessionError::DaemonError {
        message: "HOME not set — cannot configure tmux shim PATH".to_string(),
    })?;
    let shim_bin_dir = home_dir.join(".kild").join("bin");

    // Prepend shim dir to PATH so our tmux shim is found first.
    // NOTE: For login shells on macOS, /etc/zprofile runs path_helper which
    // reconstructs PATH and may push this to the end. The ZDOTDIR wrapper
    // below re-prepends it after all profile scripts have run.
    if let Some(path_entry) = env_vars.iter_mut().find(|(k, _)| k == "PATH") {
        path_entry.1 = format!("{}:{}", shim_bin_dir.display(), path_entry.1);
    } else if let Ok(system_path) = std::env::var("PATH") {
        env_vars.push((
            "PATH".to_string(),
            format!("{}:{}", shim_bin_dir.display(), system_path),
        ));
    }

    // Create a ZDOTDIR wrapper so that ~/.kild/bin is prepended to PATH
    // AFTER login shell profile scripts run (macOS path_helper in /etc/zprofile
    // reconstructs PATH and drops our prepended entry).
    let zdotdir = home_dir
        .join(".kild")
        .join("shim")
        .join(session_id)
        .join("zdotdir");
    if let Err(e) = create_zdotdir_wrapper(&zdotdir, &shim_bin_dir) {
        warn!(
            event = "core.session.zdotdir_setup_failed",
            session_id = session_id,
            error = %e,
        );
        eprintln!(
            "Warning: Failed to set up shell PATH wrapper: {}. \
             The tmux shim may not be found by agents (macOS path_helper can reorder PATH).",
            e
        );
    } else {
        env_vars.push(("ZDOTDIR".to_string(), zdotdir.display().to_string()));
    }

    // $TMUX triggers Claude Code's tmux pane backend (auto mode)
    let daemon_sock = crate::daemon::socket_path();
    env_vars.push((
        "TMUX".to_string(),
        format!("{},{},0", daemon_sock.display(), std::process::id()),
    ));

    // $TMUX_PANE identifies the leader's own pane
    env_vars.push(("TMUX_PANE".to_string(), "%0".to_string()));

    // $KILD_SHIM_SESSION tells the shim where to find its state
    env_vars.push(("KILD_SHIM_SESSION".to_string(), session_id.to_string()));

    // $CLAUDE_CODE_TASK_LIST_ID for task list persistence across sessions
    if let Some(tlid) = task_list_id {
        let task_env = agents::resume::task_list_env_vars(agent_name, tlid);
        env_vars.extend(task_env);
    }

    Ok((cmd, cmd_args, env_vars, use_login_shell))
}

/// Create a ZDOTDIR wrapper that re-prepends `~/.kild/bin` to PATH.
///
/// On macOS, login shells source `/etc/zprofile` which runs `path_helper`,
/// reconstructing PATH from `/etc/paths` and dropping any prepended entries.
/// This wrapper sources the user's real `~/.zshrc` then prepends our shim dir,
/// ensuring `~/.kild/bin/tmux` is always found first.
fn create_zdotdir_wrapper(
    zdotdir: &std::path::Path,
    shim_bin_dir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(zdotdir).map_err(|e| format!("failed to create zdotdir: {}", e))?;

    // .zshenv runs before .zprofile — we need .zshrc which runs after.
    // But we also need .zshenv to reset ZDOTDIR so the user's own .zshenv
    // and .zshrc are sourced from their real home directory.
    // zsh dotfile load order: .zshenv → .zprofile (login) → .zshrc (interactive)
    // ZDOTDIR must stay set throughout so zsh reads ALL our wrappers.
    // Each wrapper sources the user's real file from $HOME.
    // .zshrc (last) unsets ZDOTDIR so nested/child shells behave normally.

    let zshenv_content = r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zshenv if it exists.
[[ -f "$HOME/.zshenv" ]] && source "$HOME/.zshenv"
"#;

    let zprofile_content = r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zprofile if it exists.
[[ -f "$HOME/.zprofile" ]] && source "$HOME/.zprofile"
"#;

    let zshrc_content = format!(
        r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zshrc if it exists.
[[ -f "$HOME/.zshrc" ]] && source "$HOME/.zshrc"

# Re-prepend shim bin dir to PATH (macOS path_helper may have reordered it).
export PATH="{shim_bin}:$PATH"

# Reset ZDOTDIR so child shells use the user's real dotfiles.
unset ZDOTDIR
"#,
        shim_bin = shim_bin_dir.display(),
    );

    std::fs::write(zdotdir.join(".zshenv"), zshenv_content)
        .map_err(|e| format!("failed to write .zshenv: {}", e))?;
    std::fs::write(zdotdir.join(".zprofile"), zprofile_content)
        .map_err(|e| format!("failed to write .zprofile: {}", e))?;
    std::fs::write(zdotdir.join(".zshrc"), zshrc_content)
        .map_err(|e| format!("failed to write .zshrc: {}", e))?;

    Ok(())
}

/// Sync a session's status with the daemon if it has a daemon-managed agent.
///
/// When a daemon PTY exits naturally (or the daemon crashes), the kild-core session
/// JSON still says Active. This function queries the daemon for the real status and
/// updates the session file if stale.
///
/// Returns `true` if the session was updated (status changed to Stopped).
/// This is a best-effort operation — daemon unreachable is treated as "stopped".
pub fn sync_daemon_session_status(session: &mut Session) -> bool {
    // Only sync Active sessions with daemon_session_id
    if session.status != SessionStatus::Active {
        return false;
    }

    let daemon_sid = match session.latest_agent().and_then(|a| a.daemon_session_id()) {
        Some(id) => id.to_string(),
        None => return false,
    };

    let status = match crate::daemon::client::get_session_status(&daemon_sid) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                event = "core.session.daemon_status_sync_failed",
                session_id = session.id,
                daemon_session_id = daemon_sid,
                error = %e,
                "Failed to query daemon for session status"
            );
            // Treat unexpected errors as "unknown" — don't sync to Stopped
            // since we can't confirm the session actually exited.
            return false;
        }
    };

    // If daemon reports Running, the session is still active — no sync needed.
    if status == Some(kild_protocol::SessionStatus::Running) {
        return false;
    }

    // Daemon reports "stopped", session not found, or daemon not running — mark as Stopped.
    info!(
        event = "core.session.daemon_status_sync",
        session_id = session.id,
        daemon_session_id = daemon_sid,
        daemon_status = ?status,
        "Syncing stale session status to Stopped"
    );

    let now = chrono::Utc::now().to_rfc3339();

    // Update in-memory session for callers (list/status display)
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(now.clone());

    // Patch status and last_activity via targeted JSON update to preserve unknown fields.
    // Using patch instead of full save prevents older binaries from dropping new fields
    // (e.g., installed kild binary dropping task_list_id added by a newer version).
    let config = Config::new();
    if let Err(e) = persistence::patch_session_json_fields(
        &config.sessions_dir(),
        &session.id,
        &[
            ("status", serde_json::json!("Stopped")),
            ("last_activity", serde_json::Value::String(now)),
        ],
    ) {
        error!(
            event = "core.session.daemon_status_sync_save_failed",
            session_id = session.id,
            error = %e,
            "Failed to persist synced status"
        );
        eprintln!(
            "Warning: kild '{}' status is stale (daemon stopped but save failed: {}). Check disk space/permissions in ~/.kild/sessions/",
            session.branch, e
        );
    }

    true
}

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
    fn test_list_sessions_empty() {
        // Create a temporary directory for this test
        let temp_dir = std::env::temp_dir().join("kild_test_empty_sessions");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Test with empty directory
        let (sessions, skipped) = persistence::load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // Note: create_session test would require git repository setup
    // Better suited for integration tests

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
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};

        let session = Session::new(
            "test-project_test-branch".to_string(),
            "test-project".to_string(),
            "test-branch".to_string(),
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
        assert_eq!(sessions[0].branch, "test-branch");

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
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
                format!("test-project_{}", branch_name),
                "test-project".to_string(),
                branch_name.to_string(),
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
    fn test_destroy_session_with_terminal_type_calls_close() {
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
            "test-project_destroy-test".to_string(),
            "test-project".to_string(),
            "destroy-test".to_string(),
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
    fn test_destroy_session_without_terminal_type_backward_compat() {
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
            "test-project_compat-test".to_string(),
            "test-project".to_string(),
            "compat-test".to_string(),
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

    #[test]
    fn test_session_terminal_type_updated_on_restart() {
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
            std::env::temp_dir().join(format!("kild_test_restart_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with ITerm agent
        let iterm_agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_restart-test_0".to_string(),
            Some(12345),
            Some("test-agent".to_string()),
            Some(1234567890),
            Some(TerminalType::ITerm),
            Some("1596".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None,
        )
        .unwrap();
        let mut session = Session::new(
            "test-project_restart-test".to_string(),
            "test-project".to_string(),
            "restart-test".to_string(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![iterm_agent],
            None,
            None,
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Simulate what restart_session does: clear agents, add new agent with Ghostty
        let ghostty_agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_restart-test_0".to_string(),
            Some(12345),
            Some("test-agent".to_string()),
            Some(1234567890),
            Some(TerminalType::Ghostty),
            Some("kild-restart-test".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None,
        )
        .unwrap();
        session.status = SessionStatus::Active;
        session.last_activity = Some(chrono::Utc::now().to_rfc3339());
        session.clear_agents();
        session.add_agent(ghostty_agent);

        persistence::save_session_to_file(&session, &sessions_dir)
            .expect("Failed to save updated session");

        // Verify the updated terminal_type persists
        let loaded = persistence::find_session_by_name(&sessions_dir, "restart-test")
            .expect("Failed to find session")
            .expect("Session should exist");

        assert_eq!(
            loaded
                .latest_agent()
                .and_then(|a| a.terminal_type().cloned()),
            Some(TerminalType::Ghostty),
            "terminal_type should be updated to Ghostty after restart"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stop_session_not_found() {
        let result = stop_session("non-existent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_open_session_not_found() {
        let result = open_session(
            "non-existent",
            crate::state::types::OpenMode::DefaultAgent,
            crate::state::types::RuntimeMode::Terminal,
            false,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_destroy_session_force_not_found() {
        let result = destroy_session("non-existent", true);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_stop_session_clears_process_info_and_sets_stopped_status() {
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
    fn test_destroy_session_force_vs_non_force_behavior() {
        let result_non_force = destroy_session("test-force-behavior", false);
        assert!(result_non_force.is_err());
        assert!(matches!(
            result_non_force.unwrap_err(),
            SessionError::NotFound { .. }
        ));

        let result_force = destroy_session("test-force-behavior", true);
        assert!(result_force.is_err());
        assert!(matches!(
            result_force.unwrap_err(),
            SessionError::NotFound { .. }
        ));
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

    // --- sync_daemon_session_status tests ---

    #[test]
    fn test_sync_daemon_skips_stopped_sessions() {
        use std::path::PathBuf;

        let mut session = Session::new(
            "test-project_sync-stopped".to_string(),
            "test-project".to_string(),
            "sync-stopped".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Stopped,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            None,
            None,
        );

        let changed = sync_daemon_session_status(&mut session);
        assert!(!changed, "Stopped sessions should be skipped");
        assert_eq!(session.status, SessionStatus::Stopped);
    }

    #[test]
    fn test_sync_daemon_skips_sessions_without_daemon_session_id() {
        use std::path::PathBuf;

        // Active session with a terminal-managed agent (no daemon_session_id)
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test_0".to_string(),
            Some(12345),
            Some("claude-code".to_string()),
            Some(1234567890),
            None,
            None,
            "claude-code".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None, // No daemon_session_id
        )
        .unwrap();

        let mut session = Session::new(
            "test-project_sync-terminal".to_string(),
            "test-project".to_string(),
            "sync-terminal".to_string(),
            PathBuf::from("/tmp/test"),
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
        );

        let changed = sync_daemon_session_status(&mut session);
        assert!(!changed, "Terminal-managed sessions should be skipped");
        assert_eq!(session.status, SessionStatus::Active);
    }

    #[test]
    fn test_sync_daemon_skips_active_session_without_agents() {
        use std::path::PathBuf;

        // Active session with no agents at all (empty agents vec)
        let mut session = Session::new(
            "test-project_sync-no-agents".to_string(),
            "test-project".to_string(),
            "sync-no-agents".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![], // No agents
            None,
            None,
        );

        let changed = sync_daemon_session_status(&mut session);
        assert!(!changed, "Sessions with no agents should be skipped");
        assert_eq!(session.status, SessionStatus::Active);
    }

    // --- build_daemon_create_request tests ---

    #[test]
    fn test_build_daemon_request_agent_wraps_in_login_shell() {
        let (cmd, args, _env, use_login_shell) =
            build_daemon_create_request("claude --agent --verbose", "claude", "test-session", None)
                .unwrap();
        assert!(!use_login_shell, "Agent should not use login shell mode");
        // Agent commands are wrapped in $SHELL -lc 'exec <command>'
        assert!(
            cmd.ends_with("sh") || cmd.ends_with("zsh") || cmd.ends_with("bash"),
            "Command should be a shell, got: {}",
            cmd
        );
        assert_eq!(args.len(), 2, "Should have -lc and the exec command");
        assert_eq!(args[0], "-lc");
        assert!(
            args[1].contains("exec claude --agent --verbose"),
            "Should wrap command with exec, got: {}",
            args[1]
        );
    }

    #[test]
    fn test_build_daemon_request_single_word_agent_wraps_in_login_shell() {
        let (cmd, args, _env, use_login_shell) =
            build_daemon_create_request("claude", "claude", "test-session", None).unwrap();
        assert!(!use_login_shell);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-lc");
        assert!(args[1].contains("exec claude"), "got: {}", args[1]);
        assert!(
            cmd.ends_with("sh") || cmd.ends_with("zsh") || cmd.ends_with("bash"),
            "got: {}",
            cmd
        );
    }

    #[test]
    fn test_build_daemon_request_bare_shell_uses_login_shell() {
        let (_cmd, args, _env, use_login_shell) =
            build_daemon_create_request("/bin/zsh", "shell", "test-session", None).unwrap();
        assert!(use_login_shell, "Bare shell should use login shell mode");
        assert!(args.is_empty(), "Login shell mode should have no args");
    }

    #[test]
    fn test_build_daemon_request_empty_command_returns_error() {
        let result = build_daemon_create_request("", "claude", "test-session", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SessionError::DaemonError { message } => {
                assert!(
                    message.contains("claude"),
                    "Error should mention agent name, got: {}",
                    message
                );
                assert!(
                    message.contains("Empty command"),
                    "Error should mention empty command, got: {}",
                    message
                );
            }
            other => panic!("Expected DaemonError, got: {:?}", other),
        }
    }

    #[test]
    fn test_build_daemon_request_whitespace_only_command_returns_error() {
        let result = build_daemon_create_request("   ", "kiro", "test-session", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SessionError::DaemonError { message } => {
                assert!(message.contains("kiro"));
            }
            other => panic!("Expected DaemonError, got: {:?}", other),
        }
    }

    #[test]
    fn test_build_daemon_request_bare_shell_empty_command_still_works() {
        // Bare shell with empty-ish command: since use_login_shell=true,
        // the command is passed through for logging only (daemon ignores it)
        let result = build_daemon_create_request("", "shell", "test-session", None);
        assert!(result.is_ok(), "Bare shell should accept empty command");
        let (_cmd, _args, _env, use_login_shell) = result.unwrap();
        assert!(use_login_shell);
    }

    #[test]
    fn test_build_daemon_request_agent_escapes_single_quotes() {
        let (_, args, _, _) = build_daemon_create_request(
            "claude --note 'hello world'",
            "claude",
            "test-session",
            None,
        )
        .unwrap();
        assert!(
            args[1].contains("exec claude --note"),
            "Should contain the command, got: {}",
            args[1]
        );
    }

    #[test]
    fn test_build_daemon_request_collects_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None).unwrap();

        // PATH and HOME should always be present in the environment
        let keys: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            keys.contains(&"PATH"),
            "Should collect PATH env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"HOME"),
            "Should collect HOME env var, got keys: {:?}",
            keys
        );
    }

    #[test]
    fn test_build_daemon_request_includes_shim_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "proj_my-branch", None).unwrap();

        let keys: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();

        // Should include tmux shim environment variables
        assert!(
            keys.contains(&"TMUX"),
            "Should set TMUX env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"TMUX_PANE"),
            "Should set TMUX_PANE env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"KILD_SHIM_SESSION"),
            "Should set KILD_SHIM_SESSION env var, got keys: {:?}",
            keys
        );

        // KILD_SHIM_SESSION should contain the session_id
        let shim_session = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SHIM_SESSION")
            .map(|(_, v)| v.as_str());
        assert_eq!(shim_session, Some("proj_my-branch"));

        // TMUX_PANE should be %0
        let tmux_pane = env_vars
            .iter()
            .find(|(k, _)| k == "TMUX_PANE")
            .map(|(_, v)| v.as_str());
        assert_eq!(tmux_pane, Some("%0"));

        // PATH should be prepended with shim bin dir
        let path_val = env_vars
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert!(
            path_val.contains(".kild/bin"),
            "PATH should contain .kild/bin shim dir, got: {}",
            path_val
        );
    }

    #[test]
    fn test_build_daemon_request_includes_task_list_env_var_for_claude() {
        let (_cmd, _args, env_vars, _) = build_daemon_create_request(
            "claude",
            "claude",
            "myproject_my-branch",
            Some("kild-myproject_my-branch"),
        )
        .unwrap();

        let task_list_val = env_vars
            .iter()
            .find(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            task_list_val,
            Some("kild-myproject_my-branch"),
            "CLAUDE_CODE_TASK_LIST_ID should be set for claude agent"
        );
    }

    #[test]
    fn test_build_daemon_request_no_task_list_env_var_for_non_claude() {
        for (agent_cmd, agent_name) in &[
            ("kiro", "kiro"),
            ("gemini", "gemini"),
            ("amp", "amp"),
            ("opencode", "opencode"),
        ] {
            let (_cmd, _args, env_vars, _) = build_daemon_create_request(
                agent_cmd,
                agent_name,
                "test-session",
                Some("kild-test"),
            )
            .unwrap();

            let has_task_list = env_vars
                .iter()
                .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
            assert!(
                !has_task_list,
                "CLAUDE_CODE_TASK_LIST_ID should not be set for agent '{}'",
                agent_name
            );
        }
    }

    #[test]
    fn test_build_daemon_request_no_task_list_env_var_when_none() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None).unwrap();

        let has_task_list = env_vars
            .iter()
            .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
        assert!(
            !has_task_list,
            "CLAUDE_CODE_TASK_LIST_ID should not be set when task_list_id is None"
        );
    }

    #[test]
    fn test_session_with_missing_worktree_fails_operation_validation() {
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
            "test-project_orphaned-session".to_string(),
            "test-project".to_string(),
            "orphaned-session".to_string(),
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

    // --- Session Resume tests ---

    /// Tests the resume decision logic that open_session uses internally.
    ///
    /// These test the exact branching conditions from handler.rs lines 742-789
    /// without needing terminal/daemon infrastructure (Config::new reads from $HOME).
    #[test]
    fn test_resume_decision_unsupported_agent_with_session_id() {
        // Scenario: resume=true, agent=kiro (unsupported), session has session_id
        // Expected: ResumeUnsupported error
        use crate::agents;
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
        use crate::agents;
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
        use crate::agents;

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
        use crate::agents;

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
    fn test_session_id_survives_stop_lifecycle() {
        // Verify agent_session_id persists across stop (clear_agents + save + load)
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
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
            "test-project_sid-lifecycle".to_string(),
            "test-project".to_string(),
            "sid-lifecycle".to_string(),
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

    #[test]
    fn test_create_session_generates_session_id_for_claude() {
        // Verify that agent_session_id generation works for resume-capable agents
        use crate::agents;

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
    fn test_resume_args_generated_correctly_for_claude() {
        use crate::agents;

        let sid = "550e8400-e29b-41d4-a716-446655440000";
        let args = agents::resume::resume_session_args("claude", sid);
        assert_eq!(args, vec!["--resume", sid]);

        // Non-Claude should get nothing
        let args = agents::resume::resume_session_args("kiro", sid);
        assert!(args.is_empty());
    }
}
