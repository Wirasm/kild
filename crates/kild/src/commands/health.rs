use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::health;

use super::helpers::{is_valid_branch_name, load_config_with_warning};

/// Truncate a string to a maximum display width, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{:<width$}", format!("{}...", truncated), width = max_len)
    }
}

pub(crate) fn handle_health_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
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

        let cpu_str = match kild.metrics.cpu_usage_percent {
            Some(c) => format!("{:.1}%", c),
            None => "N/A".to_string(),
        };

        let mem_str = match kild.metrics.memory_usage_mb {
            Some(m) => format!("{}MB", m),
            None => "N/A".to_string(),
        };

        let activity_str = match &kild.metrics.last_activity {
            Some(a) => truncate(a, 19),
            None => "Never".to_string(),
        };

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
