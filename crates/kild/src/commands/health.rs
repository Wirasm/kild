use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::errors::KildError;
use kild_core::events;
use kild_core::health;

use super::helpers::{is_valid_branch_name, load_config_with_warning};
use crate::table::{display_width, pad};

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
                warn!(event = "cli.health_history_save_failed", error = %e);
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
            if json_output {
                let err_msg = format!("Invalid branch name: {}", branch_name);
                let boxed = super::helpers::print_json_error(&err_msg, "INVALID_BRANCH_NAME");
                error!(event = "cli.health_invalid_branch", branch = branch_name);
                return Err(boxed);
            }
            eprintln!("Invalid branch name: {}", branch_name);
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
                error!(event = "cli.health_failed", branch = branch_name, error = %e);
                events::log_app_error(&e);

                if json_output {
                    return Err(super::helpers::print_json_error(&e, e.error_code()));
                }
                eprintln!("Health check failed for '{}': {}", branch_name, e);
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
                error!(event = "cli.health_failed", error = %e);
                events::log_app_error(&e);

                if json_output {
                    return Err(super::helpers::print_json_error(&e, e.error_code()));
                }
                eprintln!("Health check failed: {}", e);
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

    // Minimum widths = header label lengths
    let mut st_w = "St".len();
    let mut branch_w = "Branch".len();
    let mut agent_w = "Agent".len();
    let mut activity_w = "Activity".len();
    let mut cpu_w = "CPU %".len();
    let mut mem_w = "Memory".len();
    let mut status_w = "Status".len();
    let mut last_activity_w = "Last Activity".len();

    // Pre-compute row data and dynamic widths in one pass
    let rows: Vec<_> = output
        .kilds
        .iter()
        .map(|kild| {
            let status_icon = match kild.metrics.status {
                health::HealthStatus::Working => "[ok]",
                health::HealthStatus::Idle => "[--]",
                health::HealthStatus::Stuck => "[!!]",
                health::HealthStatus::Crashed => "[xx]",
                health::HealthStatus::Unknown => "[??]",
            };

            let cpu_str = match kild.metrics.cpu_usage_percent {
                Some(c) => format!("{:.1}%", c),
                None => "N/A".to_string(),
            };

            let mem_str = match kild.metrics.memory_usage_mb {
                Some(m) => format!("{}MB", m),
                None => "N/A".to_string(),
            };

            let agent_activity = kild
                .agent_status
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".to_string());

            let status_str = format!("{:?}", kild.metrics.status);

            let last_activity_str = kild
                .metrics
                .last_activity
                .as_deref()
                .unwrap_or("Never")
                .to_string();

            st_w = st_w.max(display_width(status_icon));
            branch_w = branch_w.max(display_width(&kild.branch));
            agent_w = agent_w.max(display_width(&kild.agent));
            activity_w = activity_w.max(display_width(&agent_activity));
            cpu_w = cpu_w.max(display_width(&cpu_str));
            mem_w = mem_w.max(display_width(&mem_str));
            status_w = status_w.max(display_width(&status_str));
            last_activity_w = last_activity_w.max(display_width(&last_activity_str));

            (
                status_icon,
                kild.branch.clone(),
                kild.agent.clone(),
                agent_activity,
                cpu_str,
                mem_str,
                status_str,
                last_activity_str,
            )
        })
        .collect();

    println!("Health Dashboard");
    println!(
        "┌{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(st_w + 2),
        "─".repeat(branch_w + 2),
        "─".repeat(agent_w + 2),
        "─".repeat(activity_w + 2),
        "─".repeat(cpu_w + 2),
        "─".repeat(mem_w + 2),
        "─".repeat(status_w + 2),
        "─".repeat(last_activity_w + 2),
    );
    println!(
        "│ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │",
        pad("St", st_w),
        pad("Branch", branch_w),
        pad("Agent", agent_w),
        pad("Activity", activity_w),
        pad("CPU %", cpu_w),
        pad("Memory", mem_w),
        pad("Status", status_w),
        pad("Last Activity", last_activity_w),
    );
    println!(
        "├{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(st_w + 2),
        "─".repeat(branch_w + 2),
        "─".repeat(agent_w + 2),
        "─".repeat(activity_w + 2),
        "─".repeat(cpu_w + 2),
        "─".repeat(mem_w + 2),
        "─".repeat(status_w + 2),
        "─".repeat(last_activity_w + 2),
    );

    for (icon, branch, agent, activity, cpu, mem, status, last_activity) in &rows {
        println!(
            "│ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │",
            pad(icon, st_w),
            pad(branch, branch_w),
            pad(agent, agent_w),
            pad(activity, activity_w),
            pad(cpu, cpu_w),
            pad(mem, mem_w),
            pad(status, status_w),
            pad(last_activity, last_activity_w),
        );
    }

    println!(
        "└{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(st_w + 2),
        "─".repeat(branch_w + 2),
        "─".repeat(agent_w + 2),
        "─".repeat(activity_w + 2),
        "─".repeat(cpu_w + 2),
        "─".repeat(mem_w + 2),
        "─".repeat(status_w + 2),
        "─".repeat(last_activity_w + 2),
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
        health::HealthStatus::Working => "[ok]",
        health::HealthStatus::Idle => "[--]",
        health::HealthStatus::Stuck => "[!!]",
        health::HealthStatus::Crashed => "[xx]",
        health::HealthStatus::Unknown => "[??]",
    };

    let activity = kild
        .agent_status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string());

    let status_value = format!("{} {:?}", status_icon, kild.metrics.status);

    let cpu_str = match kild.metrics.cpu_usage_percent {
        Some(c) => format!("{:.1}%", c),
        None => "N/A".to_string(),
    };

    let mem_str = match kild.metrics.memory_usage_mb {
        Some(m) => format!("{} MB", m),
        None => "N/A".to_string(),
    };

    let last_active = kild
        .metrics
        .last_activity
        .as_deref()
        .unwrap_or("Never")
        .to_string();

    let rows: Vec<(&str, String)> = vec![
        ("Branch:", kild.branch.clone()),
        ("Agent:", kild.agent.clone()),
        ("Activity:", activity),
        ("Status:", status_value),
        ("Created:", kild.created_at.clone()),
        ("Worktree:", kild.worktree_path.clone()),
        ("CPU Usage:", cpu_str),
        ("Memory:", mem_str),
        ("Last Active:", last_active),
    ];

    // "Last Active:" is the longest label at 12 chars + 1 space
    let label_width = 13;

    let value_width = rows
        .iter()
        .map(|(_, v)| display_width(v.as_str()))
        .max()
        .unwrap_or(0);

    let inner_width = label_width + value_width;
    let border = "─".repeat(inner_width + 2);

    println!("Health: {}", kild.branch);
    println!("┌{}┐", border);

    for (label, value) in &rows {
        println!(
            "│ {:<label_w$}{:<value_w$} │",
            label,
            value,
            label_w = label_width,
            value_w = value_width,
        );
    }

    println!("└{}┘", border);
}
