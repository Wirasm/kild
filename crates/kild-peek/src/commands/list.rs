use clap::ArgMatches;
use kild_peek_core::events;
use kild_peek_core::window::{list_monitors, list_windows};
use tracing::{error, info};

use crate::table;

pub fn handle_list_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    match matches.subcommand() {
        Some(("windows", sub_matches)) => handle_list_windows(sub_matches),
        Some(("monitors", sub_matches)) => handle_list_monitors(sub_matches),
        _ => {
            error!(event = "peek.cli.list_subcommand_unknown");
            Err("Unknown list subcommand".into())
        }
    }
}

fn handle_list_windows(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");
    let app_filter = matches.get_one::<String>("app");

    info!(
        event = "peek.cli.list_windows_started",
        json_output = json_output,
        app_filter = ?app_filter
    );

    match list_windows() {
        Ok(windows) => {
            // Apply app filter if provided
            let filtered = apply_app_filter(windows, app_filter);

            if json_output {
                println!("{}", serde_json::to_string_pretty(&filtered)?);
            } else if filtered.is_empty() {
                print_no_windows_message(app_filter);
            } else {
                println!("Visible windows:");
                table::print_windows_table(&filtered);
            }

            info!(
                event = "peek.cli.list_windows_completed",
                count = filtered.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to list windows: {}", e);
            error!(event = "peek.cli.list_windows_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Apply app name filter to windows list
fn apply_app_filter(
    windows: Vec<kild_peek_core::window::WindowInfo>,
    app_filter: Option<&String>,
) -> Vec<kild_peek_core::window::WindowInfo> {
    let Some(app) = app_filter else {
        return windows;
    };

    let app_lower = app.to_lowercase();
    windows
        .into_iter()
        .filter(|w| w.app_name().to_lowercase().contains(&app_lower))
        .collect()
}

/// Print appropriate message when no windows are found
fn print_no_windows_message(app_filter: Option<&String>) {
    if let Some(app) = app_filter {
        info!(event = "peek.cli.list_windows_app_filter_empty", app = app);
        println!("No windows found for app filter.");
    } else {
        println!("No visible windows found.");
    }
}

fn handle_list_monitors(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");

    info!(
        event = "peek.cli.list_monitors_started",
        json_output = json_output
    );

    match list_monitors() {
        Ok(monitors) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&monitors)?);
            } else if monitors.is_empty() {
                println!("No monitors found.");
            } else {
                println!("Monitors:");
                table::print_monitors_table(&monitors);
            }

            info!(
                event = "peek.cli.list_monitors_completed",
                count = monitors.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to list monitors: {}", e);
            error!(event = "peek.cli.list_monitors_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
