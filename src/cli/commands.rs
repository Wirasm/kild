use clap::ArgMatches;
use tracing::{error, info};

use crate::cleanup;
use crate::core::events;
use crate::core::config::ShardsConfig;
use crate::health;
use crate::process;
use crate::sessions::{handler as session_handler, types::CreateSessionRequest};

/// Validate branch name to prevent injection attacks
fn is_valid_branch_name(name: &str) -> bool {
    // Allow alphanumeric, hyphens, underscores, and forward slashes
    // Prevent path traversal and special characters
    !name.is_empty() 
        && !name.contains("..")
        && !name.starts_with('/')
        && !name.ends_with('/')
        && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
        && name.len() <= 255
}

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("create", sub_matches)) => handle_create_command(sub_matches),
        Some(("list", _)) => handle_list_command(),
        Some(("destroy", sub_matches)) => handle_destroy_command(sub_matches),
        Some(("restart", sub_matches)) => handle_restart_command(sub_matches),
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
    let branch = matches.get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    
    // Load config hierarchy
    let mut config = ShardsConfig::load_hierarchy().unwrap_or_default();
    
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
        agent = config.agent.default
    );

    let request = CreateSessionRequest::new(branch.clone(), agent_override);

    match session_handler::create_session(request, &config) {
        Ok(session) => {
            println!("✅ Shard created successfully!");
            println!("   Branch: {}", session.branch);
            println!("   Agent: {}", session.agent);
            println!("   Worktree: {}", session.worktree_path.display());
            println!("   Port Range: {}-{}", session.port_range_start, session.port_range_end);
            println!("   Status: {:?}", session.status);

            info!(
                event = "cli.create_completed",
                session_id = session.id,
                branch = session.branch
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to create shard: {}", e);

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

fn handle_list_command() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.list_started");

    match session_handler::list_sessions() {
        Ok(sessions) => {
            if sessions.is_empty() {
                println!("No active shards found.");
            } else {
                const TABLE_TOP: &str = "┌──────────────────┬─────────┬─────────┬─────────────────────┬─────────────┬─────────────┬──────────────────────┐";
                const TABLE_HEADER: &str = "│ Branch           │ Agent   │ Status  │ Created             │ Port Range  │ Process     │ Command              │";
                const TABLE_SEP: &str = "├──────────────────┼─────────┼─────────┼─────────────────────┼─────────────┼─────────────┼──────────────────────┤";
                
                println!("Active shards:");
                println!("{}", TABLE_TOP);
                println!("{}", TABLE_HEADER);
                println!("{}", TABLE_SEP);

                for session in &sessions {
                    let port_range = format!("{}-{}", session.port_range_start, session.port_range_end);
                    let process_status = session.process_id.map_or("No PID".to_string(), |pid| {
                        match process::is_process_running(pid) {
                            Ok(true) => format!("Run({})", pid),
                            Ok(false) => format!("Stop({})", pid),
                            Err(e) => {
                                tracing::warn!(
                                    event = "cli.list_process_check_failed",
                                    pid = pid,
                                    session_branch = &session.branch,
                                    error = %e
                                );
                                format!("Err({})", pid)
                            }
                        }
                    });

                    println!(
                        "│ {:<16} │ {:<7} │ {:<7} │ {:<19} │ {:<11} │ {:<11} │ {:<20} │",
                        truncate(&session.branch, 16),
                        truncate(&session.agent, 7),
                        format!("{:?}", session.status).to_lowercase(),
                        truncate(&session.created_at, 19),
                        truncate(&port_range, 11),
                        truncate(&process_status, 11),
                        truncate(&session.command, 20)
                    );
                }

                const TABLE_BOTTOM: &str = "└──────────────────┴─────────┴─────────┴─────────────────────┴─────────────┴─────────────┴──────────────────────┘";
                
                println!("{}", TABLE_BOTTOM);
            }

            info!(event = "cli.list_completed", count = sessions.len());

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to list shards: {}", e);

            error!(
                event = "cli.list_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.destroy_started", branch = branch);

    match session_handler::destroy_session(branch) {
        Ok(()) => {
            println!("✅ Shard '{}' destroyed successfully!", branch);

            info!(event = "cli.destroy_completed", branch = branch);

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to destroy shard '{}': {}", branch, e);

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

fn handle_restart_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    let agent_override = matches.get_one::<String>("agent").cloned();

    info!(event = "cli.restart_started", branch = branch, agent_override = ?agent_override);

    match session_handler::restart_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ Shard '{}' restarted successfully!", branch);
            println!("   Agent: {}", session.agent);
            println!("   Process ID: {:?}", session.process_id);
            println!("   Worktree: {}", session.worktree_path.display());
            info!(event = "cli.restart_completed", branch = branch, process_id = session.process_id);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to restart shard '{}': {}", branch, e);
            error!(event = "cli.restart_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn handle_status_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.status_started", branch = branch);

    match session_handler::get_session(branch) {
        Ok(session) => {
            println!("📊 Shard Status: {}", branch);
            println!("┌─────────────────────────────────────────────────────────────┐");
            println!("│ Branch:      {:<47} │", session.branch);
            println!("│ Agent:       {:<47} │", session.agent);
            println!("│ Status:      {:<47} │", format!("{:?}", session.status).to_lowercase());
            println!("│ Created:     {:<47} │", session.created_at);
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
                        println!("│ Process:     {:<47} │", format!("Error checking PID {}: {}", pid, e));
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
            eprintln!("❌ Failed to get status for shard '{}': {}", branch, e);

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
    } else {
        cleanup::CleanupStrategy::All
    };

    match cleanup::cleanup_all_with_strategy(strategy) {
        Ok(summary) => {
            println!("✅ Cleanup completed successfully!");
            
            if summary.total_cleaned > 0 {
                println!("   Resources cleaned:");
                
                if !summary.orphaned_branches.is_empty() {
                    println!("   📦 Branches removed: {}", summary.orphaned_branches.len());
                    for branch in &summary.orphaned_branches {
                        println!("      - {}", branch);
                    }
                }
                
                if !summary.orphaned_worktrees.is_empty() {
                    println!("   📁 Worktrees removed: {}", summary.orphaned_worktrees.len());
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
    let show_all = matches.get_flag("all");
    let json_output = matches.get_flag("json");
    
    info!(
        event = "cli.health_started",
        branch = ?branch,
        show_all = show_all,
        json_output = json_output
    );
    
    if let Some(branch_name) = branch {
        // Validate branch name
        if !is_valid_branch_name(branch_name) {
            eprintln!("❌ Invalid branch name: {}", branch_name);
            error!(event = "cli.health_invalid_branch", branch = branch_name);
            return Err("Invalid branch name".into());
        }
        
        // Single shard health
        match health::get_health_single_session(branch_name) {
            Ok(shard_health) => {
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&shard_health)?);
                } else {
                    print_single_shard_health(&shard_health);
                }
                
                info!(event = "cli.health_completed", branch = branch_name);
                Ok(())
            }
            Err(e) => {
                eprintln!("❌ Failed to get health for shard '{}': {}", branch_name, e);
                error!(event = "cli.health_failed", branch = branch_name, error = %e);
                events::log_app_error(&e);
                Err(e.into())
            }
        }
    } else {
        // All shards health
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
                Ok(())
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
    if output.shards.is_empty() {
        println!("No active shards found.");
        return;
    }
    
    println!("🏥 Shard Health Dashboard");
    println!("┌────┬──────────────────┬─────────┬──────────┬──────────┬──────────┬─────────────────────┐");
    println!("│ St │ Branch           │ Agent   │ CPU %    │ Memory   │ Status   │ Last Activity       │");
    println!("├────┼──────────────────┼─────────┼──────────┼──────────┼──────────┼─────────────────────┤");
    
    for shard in &output.shards {
        let status_icon = match shard.metrics.status {
            health::HealthStatus::Working => "✅",
            health::HealthStatus::Idle => "⏸️ ",
            health::HealthStatus::Stuck => "⚠️ ",
            health::HealthStatus::Crashed => "❌",
            health::HealthStatus::Unknown => "❓",
        };
        
        let cpu_str = shard.metrics.cpu_usage_percent
            .map(|c| format!("{:.1}%", c))
            .unwrap_or_else(|| "N/A".to_string());
        
        let mem_str = shard.metrics.memory_usage_mb
            .map(|m| format!("{}MB", m))
            .unwrap_or_else(|| "N/A".to_string());
        
        let activity_str = shard.metrics.last_activity
            .as_ref()
            .map(|a| truncate(a, 19))
            .unwrap_or_else(|| "Never".to_string());
        
        println!(
            "│ {} │ {:<16} │ {:<7} │ {:<8} │ {:<8} │ {:<8} │ {:<19} │",
            status_icon,
            truncate(&shard.branch, 16),
            truncate(&shard.agent, 7),
            truncate(&cpu_str, 8),
            truncate(&mem_str, 8),
            truncate(&format!("{:?}", shard.metrics.status), 8),
            activity_str
        );
    }
    
    println!("└────┴──────────────────┴─────────┴──────────┴──────────┴──────────┴─────────────────────┘");
    println!();
    println!("Summary: {} total | {} working | {} idle | {} stuck | {} crashed",
        output.total_count,
        output.working_count,
        output.idle_count,
        output.stuck_count,
        output.crashed_count
    );
}

fn print_single_shard_health(shard: &health::ShardHealth) {
    let status_icon = match shard.metrics.status {
        health::HealthStatus::Working => "✅",
        health::HealthStatus::Idle => "⏸️ ",
        health::HealthStatus::Stuck => "⚠️ ",
        health::HealthStatus::Crashed => "❌",
        health::HealthStatus::Unknown => "❓",
    };
    
    println!("🏥 Shard Health: {}", shard.branch);
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Branch:      {:<47} │", shard.branch);
    println!("│ Agent:       {:<47} │", shard.agent);
    println!("│ Status:      {} {:<44} │", status_icon, format!("{:?}", shard.metrics.status));
    println!("│ Created:     {:<47} │", shard.created_at);
    println!("│ Worktree:    {:<47} │", truncate(&shard.worktree_path, 47));
    
    if let Some(cpu) = shard.metrics.cpu_usage_percent {
        println!("│ CPU Usage:   {:<47} │", format!("{:.1}%", cpu));
    } else {
        println!("│ CPU Usage:   {:<47} │", "N/A");
    }
    
    if let Some(mem) = shard.metrics.memory_usage_mb {
        println!("│ Memory:      {:<47} │", format!("{} MB", mem));
    } else {
        println!("│ Memory:      {:<47} │", "N/A");
    }
    
    if let Some(activity) = &shard.metrics.last_activity {
        println!("│ Last Active: {:<47} │", truncate(activity, 47));
    } else {
        println!("│ Last Active: {:<47} │", "Never");
    }
    
    println!("└─────────────────────────────────────────────────────────────┘");
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
