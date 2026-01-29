use std::path::PathBuf;

use clap::ArgMatches;
use kild_peek_core::errors::PeekError;
use tracing::{error, info};

use kild_peek_core::assert::{Assertion, run_assertion};
use kild_peek_core::diff::{DiffRequest, compare_images};
use kild_peek_core::events;
use kild_peek_core::screenshot::{CaptureRequest, ImageFormat, capture, save_to_file};
use kild_peek_core::window::{
    find_window_by_app, find_window_by_app_and_title, list_monitors, list_windows,
};

use crate::table;

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("list", sub_matches)) => handle_list_command(sub_matches),
        Some(("screenshot", sub_matches)) => handle_screenshot_command(sub_matches),
        Some(("diff", sub_matches)) => handle_diff_command(sub_matches),
        Some(("assert", sub_matches)) => handle_assert_command(sub_matches),
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}

fn handle_list_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    match matches.subcommand() {
        Some(("windows", sub_matches)) => handle_list_windows(sub_matches),
        Some(("monitors", sub_matches)) => handle_list_monitors(sub_matches),
        _ => {
            error!(event = "cli.list_subcommand_unknown");
            Err("Unknown list subcommand".into())
        }
    }
}

fn handle_list_windows(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");
    let app_filter = matches.get_one::<String>("app");

    info!(
        event = "cli.list_windows_started",
        json_output = json_output,
        app_filter = ?app_filter
    );

    match list_windows() {
        Ok(windows) => {
            // Apply app filter if provided
            let filtered: Vec<_> = if let Some(app) = app_filter {
                let app_lower = app.to_lowercase();
                windows
                    .into_iter()
                    .filter(|w| {
                        let name = w.app_name().to_lowercase();
                        name == app_lower || name.contains(&app_lower)
                    })
                    .collect()
            } else {
                windows
            };

            if json_output {
                println!("{}", serde_json::to_string_pretty(&filtered)?);
            } else if filtered.is_empty() {
                if let Some(app) = app_filter {
                    info!(event = "cli.list_windows_app_filter_empty", app = app);
                    println!("No windows found for app filter.");
                } else {
                    println!("No visible windows found.");
                }
            } else {
                println!("Visible windows:");
                table::print_windows_table(&filtered);
            }

            info!(event = "cli.list_windows_completed", count = filtered.len());
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to list windows: {}", e);
            error!(event = "cli.list_windows_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_list_monitors(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");

    info!(
        event = "cli.list_monitors_started",
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
                event = "cli.list_monitors_completed",
                count = monitors.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to list monitors: {}", e);
            error!(event = "cli.list_monitors_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_screenshot_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let window_title = matches.get_one::<String>("window");
    let window_id = matches.get_one::<u32>("window-id");
    let app_name = matches.get_one::<String>("app");
    let monitor_index = matches.get_one::<usize>("monitor");
    let output_path = matches.get_one::<String>("output");
    let base64_flag = matches.get_flag("base64");
    let format_str = matches
        .get_one::<String>("format")
        .map(|s| s.as_str())
        .unwrap_or("png");
    let quality = *matches.get_one::<u8>("quality").unwrap_or(&85);

    // Default to base64 output if no output path specified
    let use_base64 = base64_flag || output_path.is_none();

    // Determine image format
    let format = match format_str {
        "jpg" | "jpeg" => ImageFormat::Jpeg { quality },
        _ => ImageFormat::Png,
    };

    info!(
        event = "cli.screenshot_started",
        window_title = ?window_title,
        window_id = ?window_id,
        app_name = ?app_name,
        monitor_index = ?monitor_index,
        base64 = use_base64,
        format = ?format_str
    );

    // Build the capture request
    let request = if let Some(app) = app_name {
        if let Some(title) = window_title {
            // Both --app and --window: precise matching
            CaptureRequest::window_app_and_title(app, title).with_format(format)
        } else {
            // Just --app
            CaptureRequest::window_app(app).with_format(format)
        }
    } else if let Some(title) = window_title {
        CaptureRequest::window(title).with_format(format)
    } else if let Some(id) = window_id {
        CaptureRequest::window_id(*id).with_format(format)
    } else if let Some(index) = monitor_index {
        CaptureRequest::monitor(*index).with_format(format)
    } else {
        // Default to primary monitor
        CaptureRequest::primary_monitor().with_format(format)
    };

    match capture(&request) {
        Ok(result) => {
            if let Some(path) = output_path {
                let path = PathBuf::from(path);
                save_to_file(&result, &path)?;
                println!("Screenshot saved: {}", path.display());
                println!("  Size: {}x{}", result.width(), result.height());
                println!("  Format: {}", format_str);
            } else if use_base64 {
                // Output base64 to stdout
                println!("{}", result.to_base64());
            }

            info!(
                event = "cli.screenshot_completed",
                width = result.width(),
                height = result.height()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to capture screenshot: {}", e);
            error!(event = "cli.screenshot_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_diff_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let image1 = matches.get_one::<String>("image1").unwrap();
    let image2 = matches.get_one::<String>("image2").unwrap();
    let threshold_percent = *matches.get_one::<u8>("threshold").unwrap_or(&95);
    let json_output = matches.get_flag("json");

    let threshold = (threshold_percent as f64) / 100.0;

    info!(
        event = "cli.diff_started",
        image1 = image1,
        image2 = image2,
        threshold = threshold
    );

    let request = DiffRequest::new(image1, image2).with_threshold(threshold);

    match compare_images(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let status = if result.is_similar() {
                    "SIMILAR"
                } else {
                    "DIFFERENT"
                };
                println!("Image comparison: {}", status);
                println!("  Similarity: {}", result.similarity_percent());
                println!("  Threshold: {}%", threshold_percent);
                println!("  Image 1: {}x{}", result.width1(), result.height1());
                println!("  Image 2: {}x{}", result.width2(), result.height2());
            }

            info!(
                event = "cli.diff_completed",
                similarity = result.similarity(),
                is_similar = result.is_similar()
            );

            // Exit with code 1 if images are different (for CI/scripting)
            if !result.is_similar() {
                std::process::exit(1);
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to compare images: {}", e);
            error!(event = "cli.diff_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_assert_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let window_title = matches.get_one::<String>("window");
    let app_name = matches.get_one::<String>("app");
    let exists_flag = matches.get_flag("exists");
    let visible_flag = matches.get_flag("visible");
    let similar_path = matches.get_one::<String>("similar");
    let threshold_percent = *matches.get_one::<u8>("threshold").unwrap_or(&95);
    let json_output = matches.get_flag("json");

    let threshold = (threshold_percent as f64) / 100.0;

    // Resolve the window using app and/or title
    // If both --app and --window are provided, find by app+title
    // If only --app is provided, find by app
    // If only --window is provided, use existing behavior
    let resolved_title: String = if let Some(app) = app_name {
        if let Some(title) = window_title {
            // Both --app and --window: find by app+title and return actual window title
            let window = find_window_by_app_and_title(app, title).map_err(|e| {
                error!(
                    event = "cli.assert_window_resolution_failed",
                    app = app,
                    title = title,
                    error = %e,
                    error_code = e.error_code()
                );
                events::log_app_error(&e);
                format!(
                    "Window not found for app '{}' with title '{}': {}",
                    app, title, e
                )
            })?;
            window.title().to_string()
        } else {
            // Just --app: find by app and return actual window title
            let window = find_window_by_app(app).map_err(|e| {
                error!(
                    event = "cli.assert_window_resolution_failed",
                    app = app,
                    error = %e,
                    error_code = e.error_code()
                );
                events::log_app_error(&e);
                format!("Window not found for app '{}': {}", app, e)
            })?;
            window.title().to_string()
        }
    } else if let Some(title) = window_title {
        title.clone()
    } else if exists_flag || visible_flag {
        return Err("--window or --app is required with --exists/--visible".into());
    } else {
        String::new() // Will be handled by similar_path check
    };

    // Determine which assertion to run
    let assertion = if exists_flag {
        Assertion::window_exists(&resolved_title)
    } else if visible_flag {
        Assertion::window_visible(&resolved_title)
    } else if let Some(baseline_path) = similar_path {
        // For similar assertion, we need the title to capture screenshot
        if resolved_title.is_empty() && window_title.is_none() && app_name.is_none() {
            return Err("--window or --app is required with --similar".into());
        }
        // Build capture request based on what was provided
        let request = if let Some(app) = app_name {
            if let Some(title) = window_title {
                CaptureRequest::window_app_and_title(app, title)
            } else {
                CaptureRequest::window_app(app)
            }
        } else {
            CaptureRequest::window(window_title.unwrap())
        };
        let result =
            capture(&request).map_err(|e| format!("Failed to capture screenshot: {}", e))?;

        // Save to temp file
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("peek_assert_temp.png");
        save_to_file(&result, &temp_path)
            .map_err(|e| format!("Failed to save temp screenshot: {}", e))?;

        Assertion::image_similar(&temp_path, baseline_path, threshold)
    } else {
        return Err("One of --exists, --visible, or --similar must be specified".into());
    };

    info!(event = "cli.assert_started", assertion = ?assertion);

    match run_assertion(&assertion) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let status = if result.passed { "PASS" } else { "FAIL" };
                println!("Assertion: {}", status);
                println!("  {}", result.message);
            }

            info!(event = "cli.assert_completed", passed = result.passed);

            // Exit with code 1 if assertion failed
            if !result.passed {
                std::process::exit(1);
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Assertion error: {}", e);
            error!(event = "cli.assert_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would go here
    // Most command tests require actual windows/monitors
}
