use clap::ArgMatches;
use kild_peek_core::assert::{Assertion, AssertionResult, run_assertion};
use kild_peek_core::events;
use kild_peek_core::screenshot::{CaptureRequest, capture, save_to_file};
use tracing::{error, info};

use super::window_resolution::{
    resolve_window_for_capture, resolve_window_title, resolve_window_title_with_wait,
};

pub fn handle_assert_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let window_title = matches.get_one::<String>("window");
    let app_name = matches.get_one::<String>("app");
    let exists_flag = matches.get_flag("exists");
    let visible_flag = matches.get_flag("visible");
    let similar_path = matches.get_one::<String>("similar");
    let threshold_percent = *matches.get_one::<u8>("threshold").unwrap_or(&95);
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    let threshold = (threshold_percent as f64) / 100.0;

    // Resolve the window using app and/or title, with optional wait
    let resolved_title = match if wait_flag {
        resolve_window_title_with_wait(app_name, window_title, timeout_ms)
    } else {
        resolve_window_title(app_name, window_title)
    } {
        Ok(title) => title,
        Err(e) => {
            // For --exists/--visible, window-not-found is an assertion failure, not an error.
            // Print the failure output so agents and scripts get diagnostic info.
            if exists_flag || visible_flag {
                if json_output {
                    let result = AssertionResult::fail(e.to_string());
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("Assertion: FAIL");
                    println!("  {}", e);
                }
                info!(event = "peek.cli.assert_completed", passed = false);
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::process::exit(1);
            }
            return Err(e);
        }
    };

    // Validate that window/app is provided when needed
    if (exists_flag || visible_flag) && resolved_title.is_empty() {
        return Err("--window or --app is required with --exists/--visible".into());
    }

    // Determine which assertion to run
    let assertion = if exists_flag {
        Assertion::window_exists(&resolved_title)
    } else if visible_flag {
        Assertion::window_visible(&resolved_title)
    } else if let Some(baseline_path) = similar_path {
        build_similar_assertion_with_wait(
            app_name,
            window_title,
            baseline_path,
            threshold,
            wait_flag,
            timeout_ms,
        )?
    } else {
        return Err("One of --exists, --visible, or --similar must be specified".into());
    };

    info!(event = "peek.cli.assert_started", assertion = ?assertion);

    match run_assertion(&assertion) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let status = match result.passed {
                    true => "PASS",
                    false => "FAIL",
                };
                println!("Assertion: {}", status);
                println!("  {}", result.message);
            }

            info!(event = "peek.cli.assert_completed", passed = result.passed);

            // Exit with code 1 if assertion failed
            if !result.passed {
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::process::exit(1);
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Assertion error: {}", e);
            error!(event = "peek.cli.assert_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Build a similar assertion with optional wait support
fn build_similar_assertion_with_wait(
    app_name: Option<&String>,
    window_title: Option<&String>,
    baseline_path: &str,
    threshold: f64,
    wait: bool,
    timeout_ms: u64,
) -> Result<Assertion, Box<dyn std::error::Error>> {
    if app_name.is_none() && window_title.is_none() {
        return Err("--window or --app is required with --similar".into());
    }

    // Build capture request based on what was provided, with optional wait
    let request = if wait {
        // Pre-resolve window with wait, then capture by ID
        let window = resolve_window_for_capture(app_name, window_title, Some(timeout_ms))?;
        CaptureRequest::window_id(window.id())
    } else {
        // Use direct capture (window lookup happens during capture)
        match (app_name, window_title) {
            (Some(app), Some(title)) => CaptureRequest::window_app_and_title(app, title),
            (Some(app), None) => CaptureRequest::window_app(app),
            (None, Some(title)) => CaptureRequest::window(title),
            (None, None) => unreachable!(),
        }
    };

    let result = capture(&request).map_err(|e| format!("Failed to capture screenshot: {}", e))?;

    // Save to temp file
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join("peek_assert_temp.png");
    save_to_file(&result, &temp_path)
        .map_err(|e| format!("Failed to save temp screenshot: {}", e))?;

    Ok(Assertion::image_similar(
        &temp_path,
        baseline_path,
        threshold,
    ))
}
