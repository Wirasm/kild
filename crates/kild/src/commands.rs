use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::CreateSessionRequest;
use kild_core::SessionStatus;
use kild_core::cleanup;
use kild_core::config::KildConfig;
use kild_core::events;
use kild_core::health;
use kild_core::process;
use kild_core::session_ops as session_handler;

use crate::table::truncate;

/// Branch name and agent name for a successfully opened kild
type OpenedKild = (String, String);

/// Branch name and error message for a failed operation
type FailedOperation = (String, String);

/// Load configuration with warning on errors.
///
/// Falls back to defaults if config loading fails, but notifies the user via:
/// - stderr message for immediate visibility
/// - structured log event `cli.config.load_failed` for debugging
fn load_config_with_warning() -> KildConfig {
    match KildConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "Warning: Could not load config: {}. Using defaults.\n\
                 Tip: Check ~/.kild/config.toml and ./.kild/config.toml for syntax errors.",
                e
            );
            warn!(
                event = "cli.config.load_failed",
                error = %e,
                "Config load failed, using defaults"
            );
            KildConfig::default()
        }
    }
}

/// Validate branch name to prevent injection attacks
fn is_valid_branch_name(name: &str) -> bool {
    // Allow alphanumeric, hyphens, underscores, and forward slashes
    // Prevent path traversal and special characters
    !name.is_empty()
        && !name.contains("..")
        && !name.starts_with('/')
        && !name.ends_with('/')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
        && name.len() <= 255
}

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("create", sub_matches)) => handle_create_command(sub_matches),
        Some(("list", sub_matches)) => handle_list_command(sub_matches),
        Some(("cd", sub_matches)) => handle_cd_command(sub_matches),
        Some(("destroy", sub_matches)) => handle_destroy_command(sub_matches),
        Some(("complete", sub_matches)) => handle_complete_command(sub_matches),
        Some(("restart", sub_matches)) => handle_restart_command(sub_matches),
        Some(("open", sub_matches)) => handle_open_command(sub_matches),
        Some(("stop", sub_matches)) => handle_stop_command(sub_matches),
        Some(("code", sub_matches)) => handle_code_command(sub_matches),
        Some(("focus", sub_matches)) => handle_focus_command(sub_matches),
        Some(("diff", sub_matches)) => handle_diff_command(sub_matches),
        Some(("commits", sub_matches)) => handle_commits_command(sub_matches),
        Some(("status", sub_matches)) => handle_status_command(sub_matches),
        Some(("cleanup", sub_matches)) => handle_cleanup_command(sub_matches),
        Some(("health", sub_matches)) => handle_health_command(sub_matches),
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}

fn handle_create_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let note = matches.get_one::<String>("note").cloned();

    let mut config = load_config_with_warning();

    // Apply CLI overrides only if provided
    let agent_override = matches.get_one::<String>("agent").cloned();
    if let Some(agent) = &agent_override {
        config.agent.default = agent.clone();
    }
    if let Some(terminal) = matches.get_one::<String>("terminal") {
        config.terminal.preferred = Some(terminal.clone());
    }
    if let Some(startup_command) = matches.get_one::<String>("startup-command") {
        config.agent.startup_command = Some(startup_command.clone());
    }
    if let Some(flags) = matches.get_one::<String>("flags") {
        config.agent.flags = Some(flags.clone());
    }

    info!(
        event = "cli.create_started",
        branch = branch,
        agent = config.agent.default,
        note = ?note
    );

    let base_branch = matches.get_one::<String>("base").cloned();
    let no_fetch = matches.get_flag("no-fetch");

    let request = CreateSessionRequest::new(branch.clone(), agent_override, note)
        .with_base_branch(base_branch)
        .with_no_fetch(no_fetch);

    match session_handler::create_session(request, &config) {
        Ok(session) => {
            println!("✅ KILD created successfully!");
            println!("   Branch: {}", session.branch);
            println!("   Agent: {}", session.agent);
            println!("   Worktree: {}", session.worktree_path.display());
            println!(
                "   Port Range: {}-{}",
                session.port_range_start, session.port_range_end
            );
            println!("   Status: {:?}", session.status);

            info!(
                event = "cli.create_completed",
                session_id = session.id,
                branch = session.branch
            );

            Ok(())
        }
        Err(e) => {
            // Surface actionable hint for fetch failures
            let err_str = e.to_string();
            if err_str.contains("Failed to fetch") {
                eprintln!("❌ Failed to create kild: {}", e);
                eprintln!(
                    "   Hint: Use --no-fetch to skip fetching, or check your network/remote config."
                );
            } else {
                eprintln!("❌ Failed to create kild: {}", e);
            }

            error!(
                event = "cli.create_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_list_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");

    info!(event = "cli.list_started", json_output = json_output);

    match session_handler::list_sessions() {
        Ok(sessions) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else if sessions.is_empty() {
                println!("No active kilds found.");
            } else {
                println!("Active kilds:");
                let formatter = crate::table::TableFormatter::new(&sessions);
                formatter.print_table(&sessions);
            }

            info!(event = "cli.list_completed", count = sessions.len());

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to list kilds: {}", e);

            error!(
                event = "cli.list_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_cd_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    // Validate branch name (no emoji - this command is for shell integration)
    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.cd_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    info!(event = "cli.cd_started", branch = branch);

    match session_handler::get_session(branch) {
        Ok(session) => {
            // Print only the path - no formatting, no leading text
            // This enables shell integration: cd "$(kild cd branch)"
            println!("{}", session.worktree_path.display());

            info!(
                event = "cli.cd_completed",
                branch = branch,
                path = %session.worktree_path.display()
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to get path for kild '{}': {}", branch, e);

            error!(
                event = "cli.cd_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let force = matches.get_flag("force");

    if matches.get_flag("all") {
        return handle_destroy_all(force);
    }

    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(
        event = "cli.destroy_started",
        branch = branch,
        force = force
    );

    // Pre-destroy safety check (unless --force is specified)
    if !force
        && let Ok(safety_info) = session_handler::get_destroy_safety_info(branch)
        && safety_info.has_warnings()
    {
        let warnings = safety_info.warning_messages();
        for warning in &warnings {
            if safety_info.should_block() {
                eprintln!("⚠️  {}", warning);
            } else {
                println!("⚠️  {}", warning);
            }
        }

        // Block on uncommitted changes
        if safety_info.should_block() {
            eprintln!();
            eprintln!("❌ Cannot destroy '{}' with uncommitted changes.", branch);
            eprintln!("   Use --force to destroy anyway (changes will be lost).");

            error!(
                event = "cli.destroy_blocked",
                branch = branch,
                reason = "uncommitted_changes"
            );

            return Err("Uncommitted changes detected. Use --force to override.".into());
        }
    }

    match session_handler::destroy_session(branch, force) {
        Ok(()) => {
            println!("✅ KILD '{}' destroyed successfully!", branch);

            info!(event = "cli.destroy_completed", branch = branch);

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to destroy kild '{}': {}", branch, e);

            error!(
                event = "cli.destroy_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_complete_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.complete_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    let force = matches.get_flag("force");

    info!(
        event = "cli.complete_started",
        branch = branch,
        force = force
    );

    // Pre-complete safety check (always — complete never bypasses uncommitted check)
    let safety_info = match session_handler::get_destroy_safety_info(branch) {
        Ok(info) => Some(info),
        Err(e) => {
            warn!(
                event = "cli.complete_safety_check_failed",
                branch = branch,
                error = %e
            );
            None
        }
    };

    if let Some(safety_info) = &safety_info {
        if safety_info.has_warnings() {
            let warnings = safety_info.warning_messages();
            for warning in &warnings {
                if safety_info.should_block() {
                    eprintln!("⚠️  {}", warning);
                } else {
                    println!("⚠️  {}", warning);
                }
            }
        }

        if safety_info.should_block() {
            eprintln!();
            eprintln!("❌ Cannot complete '{}' with uncommitted changes.", branch);
            eprintln!("   Use 'kild destroy --force {}' to remove anyway.", branch);

            error!(
                event = "cli.complete_blocked",
                branch = branch,
                reason = "uncommitted_changes"
            );

            return Err(
                "Uncommitted changes detected. Use 'kild destroy --force' to override.".into(),
            );
        }
    }

    match session_handler::complete_session(branch, force) {
        Ok(result) => {
            use kild_core::CompleteResult;

            println!("✅ KILD '{}' completed!", branch);
            match result {
                CompleteResult::RemoteDeleted => {
                    println!("   Remote branch deleted (PR was merged)");
                }
                CompleteResult::RemoteDeleteFailed => {
                    println!("   Remote branch deletion failed (PR was merged, check logs)");
                }
                CompleteResult::PrNotMerged => {
                    println!("   Remote branch preserved (merge will delete it)");
                }
            }

            info!(
                event = "cli.complete_completed",
                branch = branch,
                result = ?result
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to complete kild '{}': {}", branch, e);

            error!(
                event = "cli.complete_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Check if user confirmation input indicates acceptance.
/// Accepts "y" or "yes" (case-insensitive).
fn is_confirmation_accepted(input: &str) -> bool {
    let normalized = input.trim().to_lowercase();
    normalized == "y" || normalized == "yes"
}

/// Format partial failure error message for bulk operations.
fn format_partial_failure_error(operation: &str, failed: usize, total: usize) -> String {
    format!(
        "Partial failure: {} of {} kild(s) failed to {}",
        failed, total, operation
    )
}

/// Handle `kild destroy --all` - destroy all kilds for current project
fn handle_destroy_all(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.destroy_all_started", force = force);

    let sessions = session_handler::list_sessions()?;

    if sessions.is_empty() {
        println!("No kilds to destroy.");
        info!(
            event = "cli.destroy_all_completed",
            destroyed = 0,
            failed = 0
        );
        return Ok(());
    }

    // Confirmation prompt unless --force is specified
    if !force {
        use std::io::{self, Write};

        print!(
            "Destroy ALL {} kild(s)? This cannot be undone. [y/N] ",
            sessions.len()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !is_confirmation_accepted(&input) {
            println!("Aborted.");
            info!(event = "cli.destroy_all_aborted");
            return Ok(());
        }
    }

    let mut destroyed: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in sessions {
        match session_handler::destroy_session(&session.branch, force) {
            Ok(()) => {
                info!(event = "cli.destroy_completed", branch = session.branch);
                destroyed.push(session.branch);
            }
            Err(e) => {
                error!(
                    event = "cli.destroy_failed",
                    branch = session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch, e.to_string()));
            }
        }
    }

    // Report successes
    if !destroyed.is_empty() {
        println!("Destroyed {} kild(s):", destroyed.len());
        for branch in &destroyed {
            println!("   {}", branch);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("Failed to destroy {} kild(s):", errors.len());
        for (branch, err) in &errors {
            eprintln!("   {}: {}", branch, err);
        }
    }

    info!(
        event = "cli.destroy_all_completed",
        destroyed = destroyed.len(),
        failed = errors.len()
    );

    // Return error if any failures (for exit code)
    if !errors.is_empty() {
        let total_count = destroyed.len() + errors.len();
        return Err(format_partial_failure_error("destroy", errors.len(), total_count).into());
    }

    Ok(())
}

fn handle_restart_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    let agent_override = matches.get_one::<String>("agent").cloned();

    eprintln!(
        "⚠️  'restart' is deprecated. Use 'kild stop {}' then 'kild open {}' for similar behavior.",
        branch, branch
    );
    eprintln!(
        "   Note: 'restart' kills the existing process. 'open' is additive (keeps existing terminals)."
    );
    warn!(event = "cli.restart_deprecated", branch = branch);
    info!(event = "cli.restart_started", branch = branch, agent_override = ?agent_override);

    match session_handler::restart_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ KILD '{}' restarted successfully!", branch);
            println!("   Agent: {}", session.agent);
            println!("   Process ID: {:?}", session.process_id);
            println!("   Worktree: {}", session.worktree_path.display());
            info!(
                event = "cli.restart_completed",
                branch = branch,
                process_id = session.process_id
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to restart kild '{}': {}", branch, e);
            error!(event = "cli.restart_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_open_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    // Check for --all flag first
    if matches.get_flag("all") {
        let agent_override = matches.get_one::<String>("agent").cloned();
        return handle_open_all(agent_override);
    }

    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;
    let agent_override = matches.get_one::<String>("agent").cloned();

    info!(event = "cli.open_started", branch = branch, agent_override = ?agent_override);

    match session_handler::open_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ Opened new agent in kild '{}'", branch);
            println!("   Agent: {}", session.agent);
            if let Some(pid) = session.process_id {
                println!("   PID: {}", pid);
            }
            info!(
                event = "cli.open_completed",
                branch = branch,
                session_id = session.id
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to open kild '{}': {}", branch, e);
            error!(event = "cli.open_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild open --all` - open agents in all stopped kilds
fn handle_open_all(agent_override: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.open_all_started", agent_override = ?agent_override);

    let sessions = session_handler::list_sessions()?;
    let stopped: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.status == SessionStatus::Stopped)
        .collect();

    if stopped.is_empty() {
        println!("No stopped kilds to open.");
        info!(event = "cli.open_all_completed", opened = 0, failed = 0);
        return Ok(());
    }

    let mut opened: Vec<OpenedKild> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in stopped {
        match session_handler::open_session(&session.branch, agent_override.clone()) {
            Ok(s) => {
                info!(
                    event = "cli.open_completed",
                    branch = s.branch,
                    session_id = s.id
                );
                opened.push((s.branch, s.agent));
            }
            Err(e) => {
                error!(
                    event = "cli.open_failed",
                    branch = session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch, e.to_string()));
            }
        }
    }

    // Report successes
    if !opened.is_empty() {
        println!("Opened {} kild(s):", opened.len());
        for (branch, agent) in &opened {
            println!("   {} ({})", branch, agent);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("Failed to open {} kild(s):", errors.len());
        for (branch, err) in &errors {
            eprintln!("   {}: {}", branch, err);
        }
    }

    info!(
        event = "cli.open_all_completed",
        opened = opened.len(),
        failed = errors.len()
    );

    // Return error if any failures (for exit code)
    if !errors.is_empty() {
        let total_count = opened.len() + errors.len();
        return Err(format!(
            "Partial failure: {} of {} kild(s) failed to open",
            errors.len(),
            total_count
        )
        .into());
    }

    Ok(())
}

fn handle_stop_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    // Check for --all flag first
    if matches.get_flag("all") {
        return handle_stop_all();
    }

    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(event = "cli.stop_started", branch = branch);

    match session_handler::stop_session(branch) {
        Ok(()) => {
            println!("✅ Stopped kild '{}'", branch);
            println!("   KILD preserved. Use 'kild open {}' to restart.", branch);
            info!(event = "cli.stop_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to stop kild '{}': {}", branch, e);
            error!(event = "cli.stop_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild stop --all` - stop all running kilds
fn handle_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.stop_all_started");

    let sessions = session_handler::list_sessions()?;
    let active: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.status == SessionStatus::Active)
        .collect();

    if active.is_empty() {
        println!("No running kilds to stop.");
        info!(event = "cli.stop_all_completed", stopped = 0, failed = 0);
        return Ok(());
    }

    let mut stopped: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in active {
        match session_handler::stop_session(&session.branch) {
            Ok(()) => {
                info!(event = "cli.stop_completed", branch = session.branch);
                stopped.push(session.branch);
            }
            Err(e) => {
                error!(
                    event = "cli.stop_failed",
                    branch = session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch, e.to_string()));
            }
        }
    }

    // Report successes
    if !stopped.is_empty() {
        println!("Stopped {} kild(s):", stopped.len());
        for branch in &stopped {
            println!("   {}", branch);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("Failed to stop {} kild(s):", errors.len());
        for (branch, err) in &errors {
            eprintln!("   {}: {}", branch, err);
        }
    }

    info!(
        event = "cli.stop_all_completed",
        stopped = stopped.len(),
        failed = errors.len()
    );

    // Return error if any failures (for exit code)
    if !errors.is_empty() {
        let total_count = stopped.len() + errors.len();
        return Err(format!(
            "Partial failure: {} of {} kild(s) failed to stop",
            errors.len(),
            total_count
        )
        .into());
    }

    Ok(())
}

/// Determine which editor to use based on precedence:
/// CLI flag > $EDITOR environment variable > "zed" (default)
fn select_editor(cli_override: Option<String>) -> String {
    cli_override
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "zed".to_string())
}

fn handle_code_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let editor_override = matches.get_one::<String>("editor").cloned();

    info!(
        event = "cli.code_started",
        branch = branch,
        editor_override = ?editor_override
    );

    // 1. Look up the session to get worktree path
    let session = match session_handler::get_session(branch) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.code_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 2. Determine editor: CLI flag > $EDITOR > "zed"
    let editor = select_editor(editor_override);

    info!(
        event = "cli.code_editor_selected",
        branch = branch,
        editor = editor
    );

    // 3. Spawn editor with worktree path
    match std::process::Command::new(&editor)
        .arg(&session.worktree_path)
        .spawn()
    {
        Ok(_) => {
            println!("✅ Opening '{}' in {}", branch, editor);
            println!("   Path: {}", session.worktree_path.display());
            info!(
                event = "cli.code_completed",
                branch = branch,
                editor = editor,
                worktree_path = %session.worktree_path.display()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to open editor '{}': {}", editor, e);
            eprintln!(
                "   Hint: Make sure '{}' is installed and in your PATH",
                editor
            );
            error!(
                event = "cli.code_failed",
                branch = branch,
                editor = editor,
                error = %e
            );
            Err(e.into())
        }
    }
}

fn handle_focus_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.focus_started", branch = branch);

    // 1. Look up the session
    let session = match session_handler::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.focus_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 2. Get terminal type and window ID
    let terminal_type = session.terminal_type.as_ref().ok_or_else(|| {
        eprintln!("❌ No terminal type recorded for kild '{}'", branch);
        error!(
            event = "cli.focus_failed",
            branch = branch,
            error = "no_terminal_type"
        );
        "No terminal type recorded for this kild"
    })?;

    let window_id = session.terminal_window_id.as_ref().ok_or_else(|| {
        eprintln!("❌ No window ID recorded for kild '{}'", branch);
        error!(
            event = "cli.focus_failed",
            branch = branch,
            error = "no_window_id"
        );
        "No window ID recorded for this kild"
    })?;

    // 3. Focus the terminal window
    match kild_core::terminal_ops::focus_terminal(terminal_type, window_id) {
        Ok(()) => {
            println!("✅ Focused kild '{}' terminal window", branch);
            info!(event = "cli.focus_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to focus terminal for '{}': {}", branch, e);
            error!(event = "cli.focus_failed", branch = branch, error = %e);
            Err(e.into())
        }
    }
}

fn handle_diff_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let staged = matches.get_flag("staged");

    info!(event = "cli.diff_started", branch = branch, staged = staged);

    // 1. Look up the session
    let session = match session_handler::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.diff_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 2. Build git diff command (with optional --staged flag)
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(&session.worktree_path);
    cmd.arg("diff");

    if staged {
        cmd.arg("--staged");
    }

    // 3. Execute git diff and wait for completion
    // Note: Output automatically appears in terminal via stdout inheritance
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to execute git diff: {}", e);
            eprintln!("   Hint: Make sure 'git' is installed and in your PATH");
            error!(
                event = "cli.diff_execution_failed",
                branch = branch,
                staged = staged,
                error = %e
            );
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // git diff exit codes:
    // 0 = no differences
    // 1 = differences found (NOT an error!)
    // 128+ = git error
    if let Some(code) = status.code()
        && code >= 128
    {
        let error_msg = format!("git diff failed with exit code {}", code);
        eprintln!("❌ {}", error_msg);
        eprintln!(
            "   Hint: Check that the worktree at {} is a valid git repository",
            session.worktree_path.display()
        );
        error!(
            event = "cli.diff_git_error",
            branch = branch,
            staged = staged,
            exit_code = code,
            worktree_path = %session.worktree_path.display()
        );
        return Err(error_msg.into());
    }

    info!(
        event = "cli.diff_completed",
        branch = branch,
        staged = staged,
        exit_code = status.code()
    );

    Ok(())
}

fn handle_commits_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let count = *matches.get_one::<usize>("count").unwrap_or(&10);

    // Validate branch name
    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.commits_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    info!(
        event = "cli.commits_started",
        branch = branch,
        count = count
    );

    let session = match session_handler::get_session(branch) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("Failed to find kild '{}': {}", branch, e);
            error!(
                event = "cli.commits_failed",
                branch = branch,
                error = %e
            );
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // Run git log in worktree directory
    let output = std::process::Command::new("git")
        .current_dir(&session.worktree_path)
        .args(["log", "--oneline", "-n", &count.to_string()])
        .output()
        .map_err(|e| {
            eprintln!(
                "Failed to execute git in '{}': {}",
                session.worktree_path.display(),
                e
            );
            eprintln!("Hint: Make sure git is installed and the worktree path is accessible.");
            error!(
                event = "cli.commits_git_spawn_failed",
                branch = branch,
                worktree_path = %session.worktree_path.display(),
                error = %e
            );
            format!("Failed to execute git: {}", e)
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Git error: {}", stderr);
        error!(
            event = "cli.commits_git_failed",
            branch = branch,
            error = %stderr
        );
        return Err(format!("git log failed: {}", stderr).into());
    }

    // Output commits to stdout, handling broken pipe gracefully
    if let Err(e) = std::io::stdout().write_all(&output.stdout) {
        // Broken pipe is expected when piped to tools like `head`
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("Failed to write output: {}", e);
            error!(
                event = "cli.commits_write_failed",
                branch = branch,
                error = %e
            );
            return Err(format!("Failed to write commits output: {}", e).into());
        }
    }

    info!(
        event = "cli.commits_completed",
        branch = branch,
        count = count
    );

    Ok(())
}

fn handle_status_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let json_output = matches.get_flag("json");

    info!(
        event = "cli.status_started",
        branch = branch,
        json_output = json_output
    );

    match session_handler::get_session(branch) {
        Ok(session) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&session)?);
                info!(
                    event = "cli.status_completed",
                    branch = branch,
                    process_id = session.process_id
                );
                return Ok(());
            }

            // Human-readable table output
            println!("📊 KILD Status: {}", branch);
            println!("┌─────────────────────────────────────────────────────────────┐");
            println!("│ Branch:      {:<47} │", session.branch);
            println!("│ Agent:       {:<47} │", session.agent);
            println!(
                "│ Status:      {:<47} │",
                format!("{:?}", session.status).to_lowercase()
            );
            println!("│ Created:     {:<47} │", session.created_at);
            if let Some(ref note) = session.note {
                println!("│ Note:        {} │", truncate(note, 47));
            }
            println!("│ Worktree:    {:<47} │", session.worktree_path.display());

            // Check process status if PID is available
            if let Some(pid) = session.process_id {
                match process::is_process_running(pid) {
                    Ok(true) => {
                        println!("│ Process:     {:<47} │", format!("Running (PID: {})", pid));

                        // Try to get process info
                        if let Ok(info) = process::get_process_info(pid) {
                            println!("│ Process Name: {:<46} │", info.name);
                            println!("│ Process Status: {:<44} │", format!("{:?}", info.status));
                        }
                    }
                    Ok(false) => {
                        println!("│ Process:     {:<47} │", format!("Stopped (PID: {})", pid));
                    }
                    Err(e) => {
                        println!(
                            "│ Process:     {:<47} │",
                            format!("Error checking PID {}: {}", pid, e)
                        );
                    }
                }
            } else {
                println!("│ Process:     {:<47} │", "No PID tracked");
            }

            println!("└─────────────────────────────────────────────────────────────┘");

            info!(
                event = "cli.status_completed",
                branch = branch,
                process_id = session.process_id
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to get status for kild '{}': {}", branch, e);

            error!(
                event = "cli.status_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_cleanup_command(sub_matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");

    let strategy = if sub_matches.get_flag("no-pid") {
        cleanup::CleanupStrategy::NoPid
    } else if sub_matches.get_flag("stopped") {
        cleanup::CleanupStrategy::Stopped
    } else if let Some(days) = sub_matches.get_one::<u64>("older-than") {
        cleanup::CleanupStrategy::OlderThan(*days)
    } else if sub_matches.get_flag("orphans") {
        cleanup::CleanupStrategy::Orphans
    } else {
        cleanup::CleanupStrategy::All
    };

    match cleanup::cleanup_all_with_strategy(strategy) {
        Ok(summary) => {
            println!("✅ Cleanup completed successfully!");

            if summary.total_cleaned > 0 {
                println!("   Resources cleaned:");

                if !summary.orphaned_branches.is_empty() {
                    println!(
                        "   📦 Branches removed: {}",
                        summary.orphaned_branches.len()
                    );
                    for branch in &summary.orphaned_branches {
                        println!("      - {}", branch);
                    }
                }

                if !summary.orphaned_worktrees.is_empty() {
                    println!(
                        "   📁 Worktrees removed: {}",
                        summary.orphaned_worktrees.len()
                    );
                    for worktree in &summary.orphaned_worktrees {
                        println!("      - {}", worktree.display());
                    }
                }

                if !summary.stale_sessions.is_empty() {
                    println!("   📄 Sessions removed: {}", summary.stale_sessions.len());
                    for session in &summary.stale_sessions {
                        println!("      - {}", session);
                    }
                }

                println!("   Total: {} resources cleaned", summary.total_cleaned);
            } else {
                println!("   No orphaned resources found.");
            }

            info!(
                event = "cli.cleanup_completed",
                total_cleaned = summary.total_cleaned
            );

            Ok(())
        }
        Err(cleanup::CleanupError::NoOrphanedResources) => {
            println!("✅ No orphaned resources found - repository is clean!");

            info!(event = "cli.cleanup_completed_no_resources");

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to cleanup resources: {}", e);

            error!(
                event = "cli.cleanup_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_health_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch");
    let json_output = matches.get_flag("json");
    let watch_mode = matches.get_flag("watch");
    let interval = *matches.get_one::<u64>("interval").unwrap_or(&5);

    info!(
        event = "cli.health_started",
        branch = ?branch,
        json_output = json_output,
        watch_mode = watch_mode,
        interval = interval
    );

    if watch_mode {
        run_health_watch_loop(branch, json_output, interval)
    } else {
        run_health_once(branch, json_output).map(|_| ())
    }
}

fn run_health_watch_loop(
    branch: Option<&String>,
    json_output: bool,
    interval_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    let config = load_config_with_warning();

    loop {
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush()?;

        let health_output = run_health_once(branch, json_output)?;

        if config.health.history_enabled
            && let Some(output) = health_output
        {
            let snapshot = health::HealthSnapshot::from(&output);
            if let Err(e) = health::save_snapshot(&snapshot) {
                info!(event = "cli.health_history_save_failed", error = %e);
            }
        }

        println!(
            "\nRefreshing every {}s. Press Ctrl+C to exit.",
            interval_secs
        );

        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
    }
}

/// Run health check once. Returns Some(HealthOutput) when checking all sessions,
/// None when checking a single branch.
fn run_health_once(
    branch: Option<&String>,
    json_output: bool,
) -> Result<Option<health::HealthOutput>, Box<dyn std::error::Error>> {
    if let Some(branch_name) = branch {
        // Validate branch name
        if !is_valid_branch_name(branch_name) {
            eprintln!("❌ Invalid branch name: {}", branch_name);
            error!(event = "cli.health_invalid_branch", branch = branch_name);
            return Err("Invalid branch name".into());
        }

        // Single kild health
        match health::get_health_single_session(branch_name) {
            Ok(kild_health) => {
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&kild_health)?);
                } else {
                    print_single_kild_health(&kild_health);
                }

                info!(event = "cli.health_completed", branch = branch_name);
                Ok(None) // Single branch doesn't return HealthOutput
            }
            Err(e) => {
                eprintln!("❌ Failed to get health for kild '{}': {}", branch_name, e);
                error!(event = "cli.health_failed", branch = branch_name, error = %e);
                events::log_app_error(&e);
                Err(e.into())
            }
        }
    } else {
        // All kilds health
        match health::get_health_all_sessions() {
            Ok(health_output) => {
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&health_output)?);
                } else {
                    print_health_table(&health_output);
                }

                info!(
                    event = "cli.health_completed",
                    total = health_output.total_count,
                    working = health_output.working_count
                );
                Ok(Some(health_output)) // Return for potential snapshot
            }
            Err(e) => {
                eprintln!("❌ Failed to get health status: {}", e);
                error!(event = "cli.health_failed", error = %e);
                events::log_app_error(&e);
                Err(e.into())
            }
        }
    }
}

fn print_health_table(output: &health::HealthOutput) {
    if output.kilds.is_empty() {
        println!("No active kilds found.");
        return;
    }

    println!("🏥 KILD Health Dashboard");
    println!(
        "┌────┬──────────────────┬─────────┬──────────┬──────────┬──────────┬─────────────────────┐"
    );
    println!(
        "│ St │ Branch           │ Agent   │ CPU %    │ Memory   │ Status   │ Last Activity       │"
    );
    println!(
        "├────┼──────────────────┼─────────┼──────────┼──────────┼──────────┼─────────────────────┤"
    );

    for kild in &output.kilds {
        let status_icon = match kild.metrics.status {
            health::HealthStatus::Working => "✅",
            health::HealthStatus::Idle => "⏸️ ",
            health::HealthStatus::Stuck => "⚠️ ",
            health::HealthStatus::Crashed => "❌",
            health::HealthStatus::Unknown => "❓",
        };

        let cpu_str = kild
            .metrics
            .cpu_usage_percent
            .map(|c| format!("{:.1}%", c))
            .unwrap_or_else(|| "N/A".to_string());

        let mem_str = kild
            .metrics
            .memory_usage_mb
            .map(|m| format!("{}MB", m))
            .unwrap_or_else(|| "N/A".to_string());

        let activity_str = kild
            .metrics
            .last_activity
            .as_ref()
            .map(|a| truncate(a, 19))
            .unwrap_or_else(|| "Never".to_string());

        println!(
            "│ {} │ {:<16} │ {:<7} │ {:<8} │ {:<8} │ {:<8} │ {:<19} │",
            status_icon,
            truncate(&kild.branch, 16),
            truncate(&kild.agent, 7),
            truncate(&cpu_str, 8),
            truncate(&mem_str, 8),
            truncate(&format!("{:?}", kild.metrics.status), 8),
            activity_str
        );
    }

    println!(
        "└────┴──────────────────┴─────────┴──────────┴──────────┴──────────┴─────────────────────┘"
    );
    println!();
    println!(
        "Summary: {} total | {} working | {} idle | {} stuck | {} crashed",
        output.total_count,
        output.working_count,
        output.idle_count,
        output.stuck_count,
        output.crashed_count
    );
}

fn print_single_kild_health(kild: &health::KildHealth) {
    let status_icon = match kild.metrics.status {
        health::HealthStatus::Working => "✅",
        health::HealthStatus::Idle => "⏸️ ",
        health::HealthStatus::Stuck => "⚠️ ",
        health::HealthStatus::Crashed => "❌",
        health::HealthStatus::Unknown => "❓",
    };

    println!("🏥 KILD Health: {}", kild.branch);
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Branch:      {:<47} │", kild.branch);
    println!("│ Agent:       {:<47} │", kild.agent);
    println!(
        "│ Status:      {} {:<44} │",
        status_icon,
        format!("{:?}", kild.metrics.status)
    );
    println!("│ Created:     {:<47} │", kild.created_at);
    println!("│ Worktree:    {:<47} │", truncate(&kild.worktree_path, 47));

    if let Some(cpu) = kild.metrics.cpu_usage_percent {
        println!("│ CPU Usage:   {:<47} │", format!("{:.1}%", cpu));
    } else {
        println!("│ CPU Usage:   {:<47} │", "N/A");
    }

    if let Some(mem) = kild.metrics.memory_usage_mb {
        println!("│ Memory:      {:<47} │", format!("{} MB", mem));
    } else {
        println!("│ Memory:      {:<47} │", "N/A");
    }

    if let Some(activity) = &kild.metrics.last_activity {
        println!("│ Last Active: {:<47} │", truncate(activity, 47));
    } else {
        println!("│ Last Active: {:<47} │", "Never");
    }

    println!("└─────────────────────────────────────────────────────────────┘");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to ensure env var tests don't run in parallel and interfere with each other
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short     ");
        assert_eq!(truncate("this-is-a-very-long-string", 10), "this-is...");
        assert_eq!(truncate("exact", 5), "exact");
    }

    #[test]
    fn test_truncate_edge_cases() {
        assert_eq!(truncate("", 5), "     ");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "...");
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Test that truncation handles multi-byte UTF-8 characters safely
        // without panicking at byte boundaries

        // Emoji are 4 bytes each
        let emoji_note = "Test 🚀 rockets";
        let result = truncate(emoji_note, 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with("..."));

        // Multiple emoji
        let multi_emoji = "🎉🎊🎁🎈🎆";
        let result = truncate(multi_emoji, 4);
        assert_eq!(result.chars().count(), 4);
        assert!(result.ends_with("..."));

        // Mixed ASCII and emoji
        let mixed = "Hello 世界 🌍";
        let result = truncate(mixed, 8);
        assert_eq!(result.chars().count(), 8);

        // CJK characters (3 bytes each)
        let cjk = "日本語テスト";
        let result = truncate(cjk, 5);
        assert_eq!(result.chars().count(), 5);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_note_display() {
        // Test truncation at the note column width (30 chars)
        let long_note = "This is a very long note that exceeds thirty characters";
        let result = truncate(long_note, 30);
        assert_eq!(result.chars().count(), 30);
        assert!(result.contains("..."));

        // Short note should be padded
        let short_note = "Short";
        let result = truncate(short_note, 30);
        assert_eq!(result.chars().count(), 30);
        assert!(!result.contains("..."));
    }

    #[test]
    fn test_load_config_with_warning_returns_valid_config() {
        // When config loads (successfully or with fallback), should return a valid config
        let config = load_config_with_warning();
        // Should not panic and return a config with non-empty default agent
        assert!(!config.agent.default.is_empty());
    }

    #[test]
    fn test_select_editor_cli_override_takes_precedence() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Even if $EDITOR is set, CLI override should win
        // SAFETY: We hold ENV_MUTEX to ensure no concurrent access
        unsafe {
            std::env::set_var("EDITOR", "vim");
        }
        let editor = select_editor(Some("code".to_string()));
        assert_eq!(editor, "code");
        unsafe {
            std::env::remove_var("EDITOR");
        }
    }

    #[test]
    fn test_select_editor_uses_env_when_no_override() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // SAFETY: We hold ENV_MUTEX to ensure no concurrent access
        unsafe {
            std::env::set_var("EDITOR", "nvim");
        }
        let editor = select_editor(None);
        assert_eq!(editor, "nvim");
        unsafe {
            std::env::remove_var("EDITOR");
        }
    }

    #[test]
    fn test_select_editor_defaults_to_zed() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // SAFETY: We hold ENV_MUTEX to ensure no concurrent access
        unsafe {
            std::env::remove_var("EDITOR");
        }
        let editor = select_editor(None);
        assert_eq!(editor, "zed");
    }

    #[test]
    fn test_select_editor_cli_override_ignores_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Verify CLI override completely ignores $EDITOR
        // SAFETY: We hold ENV_MUTEX to ensure no concurrent access
        unsafe {
            std::env::set_var("EDITOR", "emacs");
        }
        let editor = select_editor(Some("sublime".to_string()));
        assert_eq!(editor, "sublime");
        unsafe {
            std::env::remove_var("EDITOR");
        }
    }

    #[test]
    fn test_is_valid_branch_name_accepts_valid_names() {
        // Simple alphanumeric names
        assert!(is_valid_branch_name("feature-auth"));
        assert!(is_valid_branch_name("my_branch"));
        assert!(is_valid_branch_name("branch123"));

        // Names with forward slashes (git feature branches)
        assert!(is_valid_branch_name("feat/login"));
        assert!(is_valid_branch_name("feature/user/auth"));

        // Mixed valid characters
        assert!(is_valid_branch_name("fix-123_test/branch"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_empty() {
        assert!(!is_valid_branch_name(""));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_path_traversal() {
        // Path traversal attempts
        assert!(!is_valid_branch_name(".."));
        assert!(!is_valid_branch_name("foo/../bar"));
        assert!(!is_valid_branch_name("../etc/passwd"));
        assert!(!is_valid_branch_name("branch/.."));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_absolute_paths() {
        assert!(!is_valid_branch_name("/absolute"));
        assert!(!is_valid_branch_name("/etc/passwd"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_trailing_slash() {
        assert!(!is_valid_branch_name("branch/"));
        assert!(!is_valid_branch_name("feature/test/"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_special_characters() {
        // Spaces
        assert!(!is_valid_branch_name("has spaces"));

        // Shell injection characters
        assert!(!is_valid_branch_name("branch;rm -rf"));
        assert!(!is_valid_branch_name("branch|cat"));
        assert!(!is_valid_branch_name("branch&echo"));
        assert!(!is_valid_branch_name("branch`whoami`"));
        assert!(!is_valid_branch_name("branch$(pwd)"));

        // Other special characters
        assert!(!is_valid_branch_name("branch*"));
        assert!(!is_valid_branch_name("branch?"));
        assert!(!is_valid_branch_name("branch<file"));
        assert!(!is_valid_branch_name("branch>file"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_too_long() {
        let long_name = "a".repeat(256);
        assert!(!is_valid_branch_name(&long_name));

        // 255 is valid
        let max_name = "a".repeat(255);
        assert!(is_valid_branch_name(&max_name));
    }

    // Tests for destroy --all helper functions

    #[test]
    fn test_is_confirmation_accepted_yes() {
        assert!(is_confirmation_accepted("y"));
        assert!(is_confirmation_accepted("Y"));
        assert!(is_confirmation_accepted("yes"));
        assert!(is_confirmation_accepted("YES"));
        assert!(is_confirmation_accepted("Yes"));
        assert!(is_confirmation_accepted("yEs"));
    }

    #[test]
    fn test_is_confirmation_accepted_no() {
        assert!(!is_confirmation_accepted("n"));
        assert!(!is_confirmation_accepted("N"));
        assert!(!is_confirmation_accepted("no"));
        assert!(!is_confirmation_accepted("NO"));
        assert!(!is_confirmation_accepted(""));
        assert!(!is_confirmation_accepted("yess"));
        assert!(!is_confirmation_accepted("yeah"));
        assert!(!is_confirmation_accepted("nope"));
    }

    #[test]
    fn test_is_confirmation_accepted_with_whitespace() {
        assert!(is_confirmation_accepted("  y  "));
        assert!(is_confirmation_accepted("\ty\n"));
        assert!(is_confirmation_accepted("  yes  "));
        assert!(is_confirmation_accepted("\n\nyes\n"));
        assert!(!is_confirmation_accepted("  n  "));
        assert!(!is_confirmation_accepted("  "));
    }

    #[test]
    fn test_format_partial_failure_error_destroy() {
        let error = format_partial_failure_error("destroy", 2, 5);
        assert_eq!(error, "Partial failure: 2 of 5 kild(s) failed to destroy");
    }

    #[test]
    fn test_format_partial_failure_error_all_failed() {
        let error = format_partial_failure_error("destroy", 3, 3);
        assert_eq!(error, "Partial failure: 3 of 3 kild(s) failed to destroy");
    }

    #[test]
    fn test_format_partial_failure_error_one_failed() {
        let error = format_partial_failure_error("destroy", 1, 10);
        assert_eq!(error, "Partial failure: 1 of 10 kild(s) failed to destroy");
    }

    #[test]
    fn test_format_partial_failure_error_other_operations() {
        // Verify the helper works for other operations too
        let stop_error = format_partial_failure_error("stop", 1, 3);
        assert_eq!(stop_error, "Partial failure: 1 of 3 kild(s) failed to stop");

        let open_error = format_partial_failure_error("open", 2, 4);
        assert_eq!(open_error, "Partial failure: 2 of 4 kild(s) failed to open");
    }
}
