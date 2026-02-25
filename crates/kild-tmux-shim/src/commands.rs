use std::collections::HashMap;
use std::env;
use std::sync::OnceLock;

use kild_paths::KildPaths;
use kild_protocol::SessionStatus;
use tracing::{debug, error};

use crate::errors::ShimError;
use crate::ipc;
use crate::parser::*;
use crate::state::{self, PaneEntry, PaneRegistry, SessionEntry, WindowEntry};

pub fn execute(cmd: TmuxCommand<'_>) -> Result<i32, ShimError> {
    match cmd {
        TmuxCommand::Version => handle_version(),
        TmuxCommand::SplitWindow(args) => handle_split_window(args),
        TmuxCommand::SendKeys(args) => handle_send_keys(args),
        TmuxCommand::ListPanes(args) => handle_list_panes(args),
        TmuxCommand::KillPane(args) => handle_kill_pane(args),
        TmuxCommand::DisplayMessage(args) => handle_display_message(args),
        TmuxCommand::SelectPane(args) => handle_select_pane(args),
        TmuxCommand::SetOption(args) => handle_set_option(args),
        TmuxCommand::SelectLayout(args) => handle_select_layout(args),
        TmuxCommand::ResizePane(args) => handle_resize_pane(args),
        TmuxCommand::HasSession(args) => handle_has_session(args),
        TmuxCommand::NewSession(args) => handle_new_session(args),
        TmuxCommand::NewWindow(args) => handle_new_window(args),
        TmuxCommand::ListWindows(args) => handle_list_windows(args),
        TmuxCommand::BreakPane(args) => handle_break_pane(args),
        TmuxCommand::JoinPane(args) => handle_join_pane(args),
        TmuxCommand::CapturePane(args) => handle_capture_pane(args),
    }
}

fn session_id() -> Result<String, ShimError> {
    env::var("KILD_SHIM_SESSION").map_err(|_| {
        ShimError::state(
            "Not running inside a KILD daemon session. \
             This tmux binary is a shim for agent teams. \
             Use 'kild create --daemon' to start a session, \
             or use the system tmux at /usr/bin/tmux.",
        )
    })
}

fn current_pane_id() -> String {
    env::var("TMUX_PANE").unwrap_or_else(|_| "%0".to_string())
}

/// Resolve a target pane specifier to a pane ID.
///
/// Supports:
/// - `%N` - Direct pane ID (returned as-is)
/// - `session:window.%N` - Extracts `%N` from session:window.pane format
/// - Any other string or None - Falls back to current pane from `$TMUX_PANE` (or `%0`)
fn resolve_pane_id(target: Option<&str>) -> String {
    match target {
        Some(t) if t.starts_with('%') => t.to_string(),
        Some(t) => {
            // Try to extract pane from session:window.pane format
            if let Some(dot_pos) = t.rfind('.') {
                let pane_part = &t[dot_pos + 1..];
                if pane_part.starts_with('%') {
                    return pane_part.to_string();
                }
            }
            current_pane_id()
        }
        None => current_pane_id(),
    }
}

/// Base environment template — computed once, cloned per invocation.
/// Only dynamic part (TMUX_PANE) is added at the call site.
///
/// Returns an error if `KildPaths::resolve()` fails (e.g. `$HOME` not set),
/// since child processes need `~/.kild/bin` on PATH for the shim chain.
fn base_child_env() -> Result<&'static HashMap<String, String>, ShimError> {
    static ENV: OnceLock<Result<HashMap<String, String>, String>> = OnceLock::new();
    let result = ENV.get_or_init(|| {
        let copy_vars = ["PATH", "HOME", "SHELL", "USER", "LANG", "TERM"];
        let mut env_vars: HashMap<String, String> = copy_vars
            .iter()
            .filter_map(|&key| env::var(key).ok().map(|val| (key.to_string(), val)))
            .collect();

        // Ensure ~/.kild/bin is at the front of PATH so the shim stays on PATH
        let paths =
            KildPaths::resolve().map_err(|e| format!("failed to resolve KILD paths: {}", e))?;
        let kild_bin = paths.bin_dir();
        let current_path = env_vars.get("PATH").cloned().unwrap_or_default();
        let kild_bin_str = kild_bin.to_string_lossy();
        let already_present = current_path
            .split(':')
            .any(|component| component == kild_bin_str.as_ref());
        if !already_present {
            env_vars.insert(
                "PATH".to_string(),
                format!("{}:{}", kild_bin_str, current_path),
            );
        }

        // Propagate TMUX env so child processes see themselves inside "tmux"
        if let Ok(tmux) = env::var("TMUX") {
            env_vars.insert("TMUX".to_string(), tmux);
        }

        // Propagate session ID so child shim calls use the same registry
        if let Ok(sid) = env::var("KILD_SHIM_SESSION") {
            env_vars.insert("KILD_SHIM_SESSION".to_string(), sid);
        }

        Ok(env_vars)
    });
    result.as_ref().map_err(|msg| ShimError::state(msg.clone()))
}

fn build_child_env() -> Result<HashMap<String, String>, ShimError> {
    base_child_env().cloned()
}

fn shell_command() -> String {
    env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// Resolve dynamic pane variables (pane_dead, pane_pid, pane_dead_status) by querying
/// the daemon for the session status. Only called when the format string contains
/// these variables, to avoid unnecessary IPC round-trips.
fn resolve_pane_status(daemon_session_id: &str) -> (String, String, String) {
    let (status, pid, exit_code) = ipc::get_session_status(daemon_session_id);
    let pane_dead_str = match status {
        SessionStatus::Stopped => "1",
        // #[non_exhaustive]: all other statuses (Running, Creating, future) are alive
        _ => "0",
    };
    let pane_pid = pid.map(|p| p.to_string()).unwrap_or_default();
    let pane_dead_status = exit_code.map(|c| c.to_string()).unwrap_or_default();
    (pane_dead_str.to_string(), pane_pid, pane_dead_status)
}

fn expand_format(
    format: &str,
    pane_id: &str,
    session_name: &str,
    window_index: &str,
    window_name: &str,
    pane_title: &str,
) -> String {
    format
        .replace("#{pane_id}", pane_id)
        .replace("#{session_name}", session_name)
        .replace("#{window_index}", window_index)
        .replace("#{window_name}", window_name)
        .replace("#{pane_title}", pane_title)
}

/// Expand format string with daemon-aware pane status variables.
///
/// Extends `expand_format` with `#{pane_dead}`, `#{pane_pid}`, and `#{pane_dead_status}`
/// by querying the daemon for the session status. The daemon query is only performed
/// when the format string actually contains these variables.
fn expand_format_with_status(
    format: &str,
    pane_id: &str,
    session_name: &str,
    window_index: &str,
    window_name: &str,
    pane_title: &str,
    daemon_session_id: &str,
) -> String {
    // Check the original format string before substitution to avoid spurious
    // daemon queries if substituted values happen to contain format templates.
    let needs_status = format.contains("#{pane_dead}")
        || format.contains("#{pane_pid}")
        || format.contains("#{pane_dead_status}");

    let mut result = expand_format(
        format,
        pane_id,
        session_name,
        window_index,
        window_name,
        pane_title,
    );

    if needs_status {
        let (pane_dead, pane_pid, pane_dead_status) = resolve_pane_status(daemon_session_id);
        result = result
            .replace("#{pane_dead}", &pane_dead)
            .replace("#{pane_pid}", &pane_pid)
            .replace("#{pane_dead_status}", &pane_dead_status);
    }

    result
}

/// Create a new PTY via the daemon and register it in the pane registry.
/// Returns the new pane ID.
///
/// If `shell_command_parts` is non-empty, the daemon PTY runs that command directly
/// (e.g. `["claude", "--agent-type", "researcher"]`). When the command exits, the
/// daemon session transitions to `Stopped` and `#{pane_dead}` becomes `1`.
/// If empty, falls back to the user's login shell (`$SHELL`).
fn create_pty_pane(
    registry: &mut PaneRegistry,
    window_id: &str,
    shell_command_parts: &[&str],
) -> Result<String, ShimError> {
    let sid = session_id()?;
    let pane_id = state::allocate_pane_id(registry);
    let daemon_session_index = registry.next_pane_id - 1;
    let daemon_session_id = format!("{}_shim_{}", sid, daemon_session_index);

    let cwd = match env::current_dir() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            let fallback = env::var("HOME").unwrap_or_else(|_| "/".to_string());
            debug!(
                event = "shim.create_pty.cwd_fallback",
                error = %e,
                fallback = fallback.as_str(),
            );
            fallback
        }
    };

    let mut env_vars = build_child_env()?;
    // Set the new pane's TMUX_PANE
    env_vars.insert("TMUX_PANE".to_string(), pane_id.clone());

    let (cmd, cmd_args, use_login_shell) = if shell_command_parts.is_empty() {
        (shell_command(), vec![], true)
    } else {
        let cmd = shell_command_parts[0].to_string();
        let cmd_args: Vec<String> = shell_command_parts[1..]
            .iter()
            .map(ToString::to_string)
            .collect();
        (cmd, cmd_args, false)
    };

    debug!(
        event = "shim.split_window.create_pty_started",
        daemon_session_id = daemon_session_id,
        pane_id = pane_id,
        command = cmd.as_str(),
    );

    ipc::create_session(
        &daemon_session_id,
        &cwd,
        &cmd,
        &cmd_args,
        &env_vars,
        24,
        80,
        use_login_shell,
    )?;

    registry.panes.insert(
        pane_id.clone(),
        PaneEntry {
            daemon_session_id,
            title: String::new(),
            border_style: String::new(),
            window_id: window_id.to_string(),
            hidden: false,
        },
    );

    // Add pane to the window's pane list
    if let Some(window) = registry.windows.get_mut(window_id) {
        window.pane_ids.push(pane_id.clone());
    }

    debug!(
        event = "shim.split_window.create_pty_completed",
        pane_id = pane_id,
    );

    Ok(pane_id)
}

// -- Command handlers --

fn handle_version() -> Result<i32, ShimError> {
    println!("tmux 3.4");
    Ok(0)
}

fn handle_split_window(args: SplitWindowArgs<'_>) -> Result<i32, ShimError> {
    debug!(
        event = "shim.split_window_started",
        target = ?args.target,
        has_command = !args.command.is_empty(),
    );

    let sid = session_id()?;
    let mut registry = state::load(&sid)?;

    // Determine which window the target pane belongs to
    let parent_pane_id = resolve_pane_id(args.target);
    let window_id = registry
        .panes
        .get(&parent_pane_id)
        .map(|p| p.window_id.clone())
        .unwrap_or_else(|| "0".to_string());

    let pane_id = create_pty_pane(&mut registry, &window_id, &args.command)?;
    state::save(&sid, &registry)?;

    if args.print_info {
        let fmt = args.format.unwrap_or("#{pane_id}");
        let session_name = &registry.session_name;
        let window_name = registry
            .windows
            .get(&window_id)
            .map(|w| w.name.as_str())
            .unwrap_or("main");
        let output = expand_format(fmt, &pane_id, session_name, &window_id, window_name, "");
        println!("{}", output);
    }

    debug!(event = "shim.split_window_completed", pane_id = pane_id);
    Ok(0)
}

/// Translate tmux-style Ctrl key notation (`C-x`) to control characters.
///
/// Supports the standard ASCII control character range:
/// - `C-a` through `C-z` (case-insensitive) -> 0x01-0x1A
/// - Special: `C-[` (ESC), `C-\`, `C-]`, `C-^`, `C-_`, `C-?` (DEL)
///
/// Returns `None` for unsupported combinations (digits, most punctuation).
fn translate_ctrl_key(key: &str) -> Option<u8> {
    if !key.starts_with("C-") || key.len() != 3 {
        return None;
    }
    let ch = key.as_bytes()[2];
    match ch {
        b'a'..=b'z' => Some(ch - b'a' + 1),
        b'A'..=b'Z' => Some(ch - b'A' + 1),
        b'[' => Some(0x1B), // ESC
        b'\\' => Some(0x1C),
        b']' => Some(0x1D),
        b'^' => Some(0x1E),
        b'_' => Some(0x1F),
        b'?' => Some(0x7F), // DEL
        _ => None,
    }
}

fn translate_keys(keys: &[&str]) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    for key in keys {
        match *key {
            "Enter" | "C-m" => data.push(b'\n'),
            "Space" => data.push(b' '),
            "Tab" | "C-i" => data.push(b'\t'),
            "Escape" => data.push(0x1b),
            "BSpace" => data.push(0x7f),
            k if k.starts_with("C-") => match translate_ctrl_key(k) {
                Some(byte) => data.push(byte),
                None => data.extend_from_slice(key.as_bytes()),
            },
            _ => data.extend_from_slice(key.as_bytes()),
        }
    }
    data
}

fn handle_send_keys(args: SendKeysArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.send_keys_started", target = ?args.target, num_keys = args.keys.len());

    let sid = session_id()?;
    let registry = state::load_shared(&sid)?;

    let pane_id = resolve_pane_id(args.target);
    let pane = registry
        .panes
        .get(&pane_id)
        .ok_or_else(|| ShimError::state(format!("pane {} not found in registry", pane_id)))?;

    let data = translate_keys(&args.keys);

    ipc::write_stdin(&pane.daemon_session_id, &data)?;

    debug!(
        event = "shim.send_keys_completed",
        pane_id = pane_id,
        bytes = data.len()
    );
    Ok(0)
}

fn handle_list_panes(args: ListPanesArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.list_panes_started", target = ?args.target);

    let sid = session_id()?;
    let registry = state::load_shared(&sid)?;

    let fmt = args.format.unwrap_or("#{pane_id}");
    let session_name = &registry.session_name;

    // Collect panes, optionally filtering by window target
    let target_window_id = args.target.and_then(|t| {
        // Extract window index from "session:window" format
        t.split(':').nth(1).map(|s| s.to_string())
    });

    for (pane_id, pane) in &registry.panes {
        if pane.hidden {
            continue;
        }
        if let Some(ref wid) = target_window_id
            && &pane.window_id != wid
        {
            continue;
        }
        let window_name = registry
            .windows
            .get(&pane.window_id)
            .map(|w| w.name.as_str())
            .unwrap_or("");
        let output = expand_format_with_status(
            fmt,
            pane_id,
            session_name,
            &pane.window_id,
            window_name,
            &pane.title,
            &pane.daemon_session_id,
        );
        println!("{}", output);
    }

    debug!(event = "shim.list_panes_completed");
    Ok(0)
}

/// Kill a pane by destroying its daemon PTY and removing it from the registry.
///
/// Error handling: if the daemon is unreachable or the session is already gone,
/// the pane is still removed from the registry (safe). If the daemon returns
/// another error, we fail without removing to avoid orphaning a running PTY.
fn handle_kill_pane(args: KillPaneArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.kill_pane_started", target = ?args.target);

    let sid = session_id()?;
    let mut registry = state::load(&sid)?;

    let pane_id = resolve_pane_id(args.target);
    let pane = registry
        .panes
        .get(&pane_id)
        .ok_or_else(|| ShimError::state(format!("pane {} not found in registry", pane_id)))?;

    let daemon_session_id = pane.daemon_session_id.clone();

    // Destroy the daemon session
    match ipc::destroy_session(&daemon_session_id, true) {
        Ok(()) => {}
        Err(ShimError::DaemonNotRunning) => {
            debug!(
                event = "shim.kill_pane.daemon_not_running",
                daemon_session_id = daemon_session_id,
                pane_id = pane_id,
            );
        }
        Err(ShimError::IpcError { ref message }) if message.contains("session_not_found") => {
            debug!(
                event = "shim.kill_pane.session_already_gone",
                daemon_session_id = daemon_session_id,
                pane_id = pane_id,
            );
        }
        Err(e) => {
            error!(
                event = "shim.kill_pane.destroy_failed",
                daemon_session_id = daemon_session_id,
                pane_id = pane_id,
                error = %e,
            );
            return Err(ShimError::ipc(format!(
                "failed to destroy pane {}: {}",
                pane_id, e
            )));
        }
    }

    registry.remove_pane(&pane_id);
    state::save(&sid, &registry)?;

    debug!(event = "shim.kill_pane_completed", pane_id = pane_id);
    Ok(0)
}

fn handle_display_message(args: DisplayMsgArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.display_message_started", format = ?args.format);

    let fmt = args.format.unwrap_or("");
    let pane_id = current_pane_id();

    let needs_registry = fmt.contains("#{session_name}")
        || fmt.contains("#{window_index}")
        || fmt.contains("#{window_name}")
        || fmt.contains("#{pane_title}")
        || fmt.contains("#{pane_dead}")
        || fmt.contains("#{pane_pid}")
        || fmt.contains("#{pane_dead_status}");

    // For simple format strings, expand directly without loading state
    match fmt {
        "#{pane_id}" => {
            println!("{}", pane_id);
        }
        _ if needs_registry => {
            let sid = session_id()?;
            let registry = state::load_shared(&sid)?;
            let session_name = &registry.session_name;
            let pane_entry = registry.panes.get(&pane_id);
            let window_id = pane_entry.map(|p| p.window_id.as_str()).unwrap_or("0");
            let window_name = registry
                .windows
                .get(window_id)
                .map(|w| w.name.as_str())
                .unwrap_or("main");
            let pane_title = pane_entry.map(|p| p.title.as_str()).unwrap_or("");
            // Only query the daemon when the pane is in the registry.
            // The leader pane (%0) is not registered, so its daemon_session_id
            // would be empty — querying with an empty ID would misreport it as dead.
            let daemon_session_id = pane_entry
                .map(|p| p.daemon_session_id.as_str())
                .filter(|id| !id.is_empty());
            let output = if let Some(daemon_session_id) = daemon_session_id {
                expand_format_with_status(
                    fmt,
                    &pane_id,
                    session_name,
                    window_id,
                    window_name,
                    pane_title,
                    daemon_session_id,
                )
            } else {
                expand_format(
                    fmt,
                    &pane_id,
                    session_name,
                    window_id,
                    window_name,
                    pane_title,
                )
            };
            println!("{}", output);
        }
        _ => {
            // Print literal or unknown format as-is
            println!("{}", fmt);
        }
    }

    debug!(event = "shim.display_message_completed");
    Ok(0)
}

fn handle_select_pane(args: SelectPaneArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.select_pane_started", target = ?args.target);

    // Only need state if we have style or title to store
    if args.style.is_some() || args.title.is_some() {
        let sid = session_id()?;
        let mut registry = state::load(&sid)?;
        let pane_id = resolve_pane_id(args.target);

        if let Some(pane) = registry.panes.get_mut(&pane_id) {
            if let Some(style) = args.style {
                pane.border_style = style.to_string();
            }
            if let Some(title) = args.title {
                pane.title = title.to_string();
            }
            state::save(&sid, &registry)?;
        }
    }

    // Focus is a no-op in the shim
    debug!(event = "shim.select_pane_completed");
    Ok(0)
}

fn handle_set_option(args: SetOptionArgs<'_>) -> Result<i32, ShimError> {
    debug!(
        event = "shim.set_option_started",
        key = args.key,
        value = args.value,
        scope = ?args.scope,
    );

    // Store pane-scoped options in the pane entry
    if matches!(args.scope, OptionScope::Pane) {
        let sid = session_id()?;
        let mut registry = state::load(&sid)?;
        let pane_id = resolve_pane_id(args.target);

        if let Some(pane) = registry.panes.get_mut(&pane_id) {
            // Store known pane options
            if args.key == "pane-border-style" || args.key.ends_with("-style") {
                pane.border_style = args.value;
            }
            state::save(&sid, &registry)?;
        }
    }

    // Window/session options are no-ops
    debug!(event = "shim.set_option_completed");
    Ok(0)
}

fn handle_select_layout(_args: SelectLayoutArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.select_layout_started");
    // No-op: layout is meaningless without a real terminal multiplexer
    debug!(event = "shim.select_layout_completed");
    Ok(0)
}

fn handle_resize_pane(_args: ResizePaneArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.resize_pane_started");
    // MVP: no-op. Could send resize_pty IPC in the future.
    debug!(event = "shim.resize_pane_completed");
    Ok(0)
}

fn handle_has_session(args: HasSessionArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.has_session_started", target = args.target);

    let sid = session_id()?;
    let registry = state::load_shared(&sid)?;

    let exists = registry.sessions.contains_key(args.target);

    debug!(
        event = "shim.has_session_completed",
        target = args.target,
        exists = exists
    );
    if exists { Ok(0) } else { Ok(1) }
}

fn handle_new_session(args: NewSessionArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.new_session_started", name = ?args.session_name);

    let sid = session_id()?;
    let mut registry = state::load(&sid)?;

    let session_name = args
        .session_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("kild_{}", registry.sessions.len()));

    let window_name = args
        .window_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| "main".to_string());

    // Allocate a new window
    let window_id = format!("{}", registry.windows.len());
    registry.windows.insert(
        window_id.clone(),
        WindowEntry {
            name: window_name,
            pane_ids: vec![],
        },
    );

    // Create initial pane in the new window
    let pane_id = create_pty_pane(&mut registry, &window_id, &[])?;

    // Register session
    registry.sessions.insert(
        session_name.clone(),
        SessionEntry {
            name: session_name.clone(),
            windows: vec![window_id],
        },
    );

    state::save(&sid, &registry)?;

    if args.print_info {
        let fmt = args.format.unwrap_or("#{pane_id}");
        let output = expand_format(fmt, &pane_id, &session_name, "0", "main", "");
        println!("{}", output);
    }

    debug!(
        event = "shim.new_session_completed",
        session_name = session_name
    );
    Ok(0)
}

fn handle_new_window(args: NewWindowArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.new_window_started", name = ?args.name);

    let sid = session_id()?;
    let mut registry = state::load(&sid)?;

    let window_name = args
        .name
        .map(|s| s.to_string())
        .unwrap_or_else(|| "window".to_string());
    let window_id = format!("{}", registry.windows.len());

    registry.windows.insert(
        window_id.clone(),
        WindowEntry {
            name: window_name.clone(),
            pane_ids: vec![],
        },
    );

    let pane_id = create_pty_pane(&mut registry, &window_id, &[])?;

    // Add window to the target session (or default session)
    let session_key = args
        .target
        .and_then(|t| t.split(':').next())
        .unwrap_or(&registry.session_name)
        .to_string();

    if let Some(session) = registry.sessions.get_mut(&session_key) {
        session.windows.push(window_id.clone());
    }

    state::save(&sid, &registry)?;

    if args.print_info {
        let fmt = args.format.unwrap_or("#{pane_id}");
        let output = expand_format(fmt, &pane_id, &session_key, &window_id, &window_name, "");
        println!("{}", output);
    }

    debug!(
        event = "shim.new_window_completed",
        window_id = window_id,
        pane_id = pane_id
    );
    Ok(0)
}

fn handle_list_windows(args: ListWindowsArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.list_windows_started", target = ?args.target);

    let sid = session_id()?;
    let registry = state::load_shared(&sid)?;

    let fmt = args.format.unwrap_or("#{window_name}");
    let session_name = &registry.session_name;

    // Filter by session if target given
    let session_windows: Option<&Vec<String>> = args
        .target
        .and_then(|t| {
            let sname = t.split(':').next().unwrap_or(t);
            registry.sessions.get(sname)
        })
        .map(|s| &s.windows);

    for (window_id, window) in &registry.windows {
        if let Some(sw) = session_windows
            && !sw.contains(window_id)
        {
            continue;
        }
        let output = expand_format(fmt, "", session_name, window_id, &window.name, "");
        println!("{}", output);
    }

    debug!(event = "shim.list_windows_completed");
    Ok(0)
}

fn handle_break_pane(args: BreakPaneArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.break_pane_started", source = ?args.source);

    let sid = session_id()?;
    let mut registry = state::load(&sid)?;

    let pane_id = resolve_pane_id(args.source);
    if let Some(pane) = registry.panes.get_mut(&pane_id) {
        pane.hidden = true;
    }
    state::save(&sid, &registry)?;

    debug!(event = "shim.break_pane_completed", pane_id = pane_id);
    Ok(0)
}

fn handle_join_pane(args: JoinPaneArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.join_pane_started", source = ?args.source);

    let sid = session_id()?;
    let mut registry = state::load(&sid)?;

    let pane_id = resolve_pane_id(args.source);
    if let Some(pane) = registry.panes.get_mut(&pane_id) {
        pane.hidden = false;
    }
    state::save(&sid, &registry)?;

    debug!(event = "shim.join_pane_completed", pane_id = pane_id);
    Ok(0)
}

fn handle_capture_pane(args: CapturePaneArgs<'_>) -> Result<i32, ShimError> {
    debug!(event = "shim.capture_pane_started", target = ?args.target);

    if !args.print {
        debug!(
            event = "shim.capture_pane_completed",
            pane_id = "unknown",
            bytes = 0
        );
        return Ok(0);
    }

    let sid = session_id()?;
    let registry = state::load_shared(&sid)?;

    let pane_id = resolve_pane_id(args.target);
    let pane = registry
        .panes
        .get(&pane_id)
        .ok_or_else(|| ShimError::state(format!("pane {} not found in registry", pane_id)))?;

    let raw = ipc::read_scrollback(&pane.daemon_session_id)?;
    let text = String::from_utf8_lossy(&raw);

    let output = match args.start_line {
        Some(n) if n < 0 => {
            let lines: Vec<&str> = text.lines().collect();
            let count = (-n) as usize;
            let skip = lines.len().saturating_sub(count);
            lines[skip..].join("\n")
        }
        _ => text.into_owned(),
    };

    print!("{}", output);

    debug!(
        event = "shim.capture_pane_completed",
        pane_id = pane_id,
        bytes = raw.len()
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- translate_ctrl_key tests --

    #[test]
    fn test_translate_ctrl_key_lowercase() {
        assert_eq!(translate_ctrl_key("C-a"), Some(0x01));
        assert_eq!(translate_ctrl_key("C-c"), Some(0x03));
        assert_eq!(translate_ctrl_key("C-z"), Some(0x1A));
    }

    #[test]
    fn test_translate_ctrl_key_uppercase() {
        assert_eq!(translate_ctrl_key("C-A"), Some(0x01));
        assert_eq!(translate_ctrl_key("C-Z"), Some(0x1A));
    }

    #[test]
    fn test_translate_ctrl_key_special() {
        assert_eq!(translate_ctrl_key("C-["), Some(0x1B)); // ESC
        assert_eq!(translate_ctrl_key("C-]"), Some(0x1D));
        assert_eq!(translate_ctrl_key("C-?"), Some(0x7F)); // DEL
    }

    #[test]
    fn test_translate_ctrl_key_invalid() {
        assert_eq!(translate_ctrl_key("C-"), None); // too short
        assert_eq!(translate_ctrl_key("C-ab"), None); // too long
        assert_eq!(translate_ctrl_key("C-1"), None); // digit
        assert_eq!(translate_ctrl_key("X-a"), None); // wrong prefix
    }

    // -- translate_keys tests --

    #[test]
    fn test_translate_enter() {
        let keys = vec!["Enter"];
        assert_eq!(translate_keys(&keys), b"\n");
    }

    #[test]
    fn test_translate_space() {
        let keys = vec!["Space"];
        assert_eq!(translate_keys(&keys), b" ");
    }

    #[test]
    fn test_translate_tab() {
        let keys = vec!["Tab"];
        assert_eq!(translate_keys(&keys), b"\t");
    }

    #[test]
    fn test_translate_escape() {
        let keys = vec!["Escape"];
        assert_eq!(translate_keys(&keys), vec![0x1b]);
    }

    #[test]
    fn test_translate_bspace() {
        let keys = vec!["BSpace"];
        assert_eq!(translate_keys(&keys), vec![0x7f]);
    }

    #[test]
    fn test_translate_c_m_alias() {
        let keys = vec!["C-m"];
        assert_eq!(translate_keys(&keys), b"\n");
    }

    #[test]
    fn test_translate_c_i_alias() {
        let keys = vec!["C-i"];
        assert_eq!(translate_keys(&keys), b"\t");
    }

    #[test]
    fn test_translate_unknown_key_passthrough() {
        let keys = vec!["hello"];
        assert_eq!(translate_keys(&keys), b"hello");
    }

    #[test]
    fn test_translate_empty_keys() {
        let keys: Vec<&str> = vec![];
        assert_eq!(translate_keys(&keys), b"");
    }

    #[test]
    fn test_translate_literal_text_with_enter() {
        let keys = vec!["echo", "Space", "hello", "Enter"];
        assert_eq!(translate_keys(&keys), b"echo hello\n");
    }

    #[test]
    fn test_translate_long_command() {
        let keys = vec!["ls", "Space", "-la", "Space", "/tmp", "Enter"];
        assert_eq!(translate_keys(&keys), b"ls -la /tmp\n");
    }

    // -- capture_pane output formatting tests --

    /// Helper: simulates the capture-pane output logic from handle_capture_pane
    fn format_capture_output(raw: &[u8], start_line: Option<i64>) -> String {
        let text = String::from_utf8_lossy(raw);
        match start_line {
            Some(n) if n < 0 => {
                let lines: Vec<&str> = text.lines().collect();
                let count = (-n) as usize;
                let skip = lines.len().saturating_sub(count);
                lines[skip..].join("\n")
            }
            _ => text.into_owned(),
        }
    }

    #[test]
    fn test_capture_pane_empty_scrollback() {
        let output = format_capture_output(b"", None);
        assert_eq!(output, "");
    }

    #[test]
    fn test_capture_pane_empty_scrollback_with_start_line() {
        let output = format_capture_output(b"", Some(-10));
        assert_eq!(output, "");
    }

    #[test]
    fn test_capture_pane_start_line_exceeds_buffer() {
        let raw = b"line1\nline2\nline3";
        let output = format_capture_output(raw, Some(-100));
        assert_eq!(output, "line1\nline2\nline3");
    }

    #[test]
    fn test_capture_pane_start_line_last_two() {
        let raw = b"line1\nline2\nline3\nline4\nline5";
        let output = format_capture_output(raw, Some(-2));
        assert_eq!(output, "line4\nline5");
    }

    #[test]
    fn test_capture_pane_start_line_last_one() {
        let raw = b"first\nsecond\nthird";
        let output = format_capture_output(raw, Some(-1));
        assert_eq!(output, "third");
    }

    #[test]
    fn test_capture_pane_invalid_utf8() {
        let raw: Vec<u8> = vec![
            b'h', b'e', b'l', b'l', b'o', b'\n', 0xFF, 0xFE, b'\n', b'w', b'o', b'r', b'l', b'd',
        ];
        let output = format_capture_output(&raw, Some(-2));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1], "world");
    }
}
