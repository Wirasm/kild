use std::path::PathBuf;

use clap::ArgMatches;
use kild_peek_core::errors::PeekError;
use tracing::{error, info, warn};

use kild_peek_core::assert::{Assertion, run_assertion};
use kild_peek_core::diff::{DiffRequest, compare_images};
use kild_peek_core::element::{ElementsRequest, FindRequest, find_element, list_elements};
use kild_peek_core::events;
use kild_peek_core::interact::{
    ClickModifier, ClickRequest, ClickTextRequest, DragRequest, HoverRequest, HoverTextRequest,
    InteractionTarget, KeyComboRequest, ScrollRequest, TypeRequest, click, click_text, drag, hover,
    hover_text, scroll, send_key_combo, type_text,
};
use kild_peek_core::screenshot::{CaptureRequest, CropArea, ImageFormat, capture, save_to_file};
use kild_peek_core::window::{
    find_window_by_app, find_window_by_app_and_title, find_window_by_app_and_title_with_wait,
    find_window_by_app_with_wait, find_window_by_title_with_wait, list_monitors, list_windows,
};

use crate::table;

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("list", sub_matches)) => handle_list_command(sub_matches),
        Some(("screenshot", sub_matches)) => handle_screenshot_command(sub_matches),
        Some(("diff", sub_matches)) => handle_diff_command(sub_matches),
        Some(("elements", sub_matches)) => handle_elements_command(sub_matches),
        Some(("find", sub_matches)) => handle_find_command(sub_matches),
        Some(("click", sub_matches)) => handle_click_command(sub_matches),
        Some(("drag", sub_matches)) => handle_drag_command(sub_matches),
        Some(("scroll", sub_matches)) => handle_scroll_command(sub_matches),
        Some(("hover", sub_matches)) => handle_hover_command(sub_matches),
        Some(("type", sub_matches)) => handle_type_command(sub_matches),
        Some(("key", sub_matches)) => handle_key_command(sub_matches),
        Some(("assert", sub_matches)) => handle_assert_command(sub_matches),
        _ => {
            error!(event = "peek.cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}

fn handle_list_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

fn handle_screenshot_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let window_title = matches.get_one::<String>("window");
    let window_id = matches.get_one::<u32>("window-id");
    let app_name = matches.get_one::<String>("app");
    let monitor_index = matches.get_one::<usize>("monitor");
    let output_path = matches.get_one::<String>("output");
    let base64_flag = matches.get_flag("base64");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
    let format_str = matches
        .get_one::<String>("format")
        .map(|s| s.as_str())
        .unwrap_or("png");
    let quality = *matches.get_one::<u8>("quality").unwrap_or(&85);
    let crop_str = matches.get_one::<String>("crop");

    // Parse crop area if provided
    let crop = match crop_str {
        Some(s) => Some(parse_crop_area(s)?),
        None => None,
    };

    // Default to base64 output if no output path specified
    let use_base64 = base64_flag || output_path.is_none();

    // Determine image format
    let format = match format_str {
        "jpg" | "jpeg" => ImageFormat::Jpeg { quality },
        _ => ImageFormat::Png,
    };

    info!(
        event = "peek.cli.screenshot_started",
        window_title = ?window_title,
        window_id = ?window_id,
        app_name = ?app_name,
        monitor_index = ?monitor_index,
        base64 = use_base64,
        format = ?format_str,
        wait = wait_flag,
        timeout_ms = timeout_ms,
        crop = ?crop
    );

    // Build the capture request, using wait functions if --wait is set
    let request = build_capture_request_with_wait(
        app_name,
        window_title,
        window_id,
        monitor_index,
        format,
        wait_flag,
        timeout_ms,
        crop,
    )?;

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
                event = "peek.cli.screenshot_completed",
                width = result.width(),
                height = result.height()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to capture screenshot: {}", e);
            error!(event = "peek.cli.screenshot_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Build a capture request from command-line arguments, with optional wait support
#[allow(clippy::too_many_arguments)]
fn build_capture_request_with_wait(
    app_name: Option<&String>,
    window_title: Option<&String>,
    window_id: Option<&u32>,
    monitor_index: Option<&usize>,
    format: ImageFormat,
    wait: bool,
    timeout_ms: u64,
    crop: Option<CropArea>,
) -> Result<CaptureRequest, Box<dyn std::error::Error>> {
    // Check if wait flag is applicable to this target
    if wait {
        if let Some(id) = window_id {
            warn!(
                event = "peek.cli.screenshot_wait_ignored",
                window_id = id,
                reason = "window-id targets are already resolved"
            );
            eprintln!(
                "Warning: --wait flag is ignored when using --window-id (window ID is already resolved)"
            );
        } else if let Some(index) = monitor_index {
            warn!(
                event = "peek.cli.screenshot_wait_ignored",
                monitor_index = index,
                reason = "monitor targets are already resolved"
            );
            eprintln!(
                "Warning: --wait flag is ignored when using --monitor (monitors don't appear dynamically)"
            );
        } else if app_name.is_some() || window_title.is_some() {
            // Wait is applicable and enabled - pre-resolve window
            let window = resolve_window_for_capture(app_name, window_title, Some(timeout_ms))?;
            let req = CaptureRequest::window_id(window.id()).with_format(format);
            return Ok(match crop {
                Some(c) => req.with_crop(c),
                None => req,
            });
        }
    }

    // No wait, or non-waitable target - use normal request building
    Ok(build_capture_request(
        app_name,
        window_title,
        window_id,
        monitor_index,
        format,
        crop,
    ))
}

/// Parse a crop area string in the format "x,y,width,height"
fn parse_crop_area(s: &str) -> Result<CropArea, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return Err("Crop format must be x,y,width,height".into());
    }
    let x: u32 = parts[0].trim().parse()?;
    let y: u32 = parts[1].trim().parse()?;
    let width: u32 = parts[2].trim().parse()?;
    let height: u32 = parts[3].trim().parse()?;
    Ok(CropArea::new(x, y, width, height))
}

/// Build a capture request from command-line arguments
fn build_capture_request(
    app_name: Option<&String>,
    window_title: Option<&String>,
    window_id: Option<&u32>,
    monitor_index: Option<&usize>,
    format: ImageFormat,
    crop: Option<CropArea>,
) -> CaptureRequest {
    let base = match (app_name, window_title, window_id, monitor_index) {
        (Some(app), Some(title), None, None) => {
            CaptureRequest::window_app_and_title(app, title).with_format(format)
        }
        (Some(app), None, None, None) => CaptureRequest::window_app(app).with_format(format),
        (None, Some(title), None, None) => CaptureRequest::window(title).with_format(format),
        (None, None, Some(id), None) => CaptureRequest::window_id(*id).with_format(format),
        (None, None, None, Some(index)) => CaptureRequest::monitor(*index).with_format(format),
        _ => CaptureRequest::primary_monitor().with_format(format),
    };

    match crop {
        Some(c) => base.with_crop(c),
        None => base,
    }
}

fn handle_diff_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let image1 = matches.get_one::<String>("image1").unwrap();
    let image2 = matches.get_one::<String>("image2").unwrap();
    let threshold_percent = *matches.get_one::<u8>("threshold").unwrap_or(&95);
    let json_output = matches.get_flag("json");
    let diff_output = matches.get_one::<String>("diff-output");

    let threshold = (threshold_percent as f64) / 100.0;

    info!(
        event = "peek.cli.diff_started",
        image1 = image1,
        image2 = image2,
        threshold = threshold,
        diff_output = ?diff_output
    );

    let mut request = DiffRequest::new(image1, image2).with_threshold(threshold);
    if let Some(path) = diff_output {
        request = request.with_diff_output(path);
    }

    match compare_images(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let status = match result.is_similar() {
                    true => "SIMILAR",
                    false => "DIFFERENT",
                };
                println!("Image comparison: {}", status);
                println!("  Similarity: {}", result.similarity_percent());
                println!("  Threshold: {}%", threshold_percent);
                println!("  Image 1: {}x{}", result.width1(), result.height1());
                println!("  Image 2: {}x{}", result.width2(), result.height2());
                if let Some(path) = result.diff_output_path() {
                    println!("  Diff saved: {}", path);
                }
            }

            info!(
                event = "peek.cli.diff_completed",
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
            error!(event = "peek.cli.diff_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Parse an InteractionTarget from --window and --app arguments
fn parse_interaction_target(
    matches: &ArgMatches,
) -> Result<InteractionTarget, Box<dyn std::error::Error>> {
    let window_title = matches.get_one::<String>("window");
    let app_name = matches.get_one::<String>("app");

    match (app_name, window_title) {
        (Some(app), Some(title)) => Ok(InteractionTarget::AppAndWindow {
            app: app.clone(),
            title: title.clone(),
        }),
        (Some(app), None) => Ok(InteractionTarget::App { app: app.clone() }),
        (None, Some(title)) => Ok(InteractionTarget::Window {
            title: title.clone(),
        }),
        (None, None) => Err("At least one of --window or --app is required".into()),
    }
}

fn parse_coordinates(at_str: &str) -> Result<(i32, i32), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = at_str.split(',').collect();
    if parts.len() != 2 {
        return Err(format!(
            "--at format must be x,y (e.g., \"100,50\"), got: '{}'",
            at_str
        )
        .into());
    }
    let x: i32 = parts[0].trim().parse().map_err(|e| {
        format!(
            "Invalid x coordinate '{}': {} (expected integer)",
            parts[0].trim(),
            e
        )
    })?;
    let y: i32 = parts[1].trim().parse().map_err(|e| {
        format!(
            "Invalid y coordinate '{}': {} (expected integer)",
            parts[1].trim(),
            e
        )
    })?;
    Ok((x, y))
}

fn handle_elements_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let json_output = matches.get_flag("json");
    let tree_output = matches.get_flag("tree");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    info!(
        event = "peek.cli.elements_started",
        target = ?target,
        tree = tree_output,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let request = if wait_flag {
        ElementsRequest::new(target).with_wait(timeout_ms)
    } else {
        ElementsRequest::new(target)
    };

    match list_elements(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if result.elements().is_empty() {
                println!("No elements found in window \"{}\"", result.window());
            } else {
                println!(
                    "Elements in \"{}\" ({} found):",
                    result.window(),
                    result.count()
                );
                if tree_output {
                    table::print_elements_tree(result.elements());
                } else {
                    table::print_elements_table(result.elements());
                }
            }

            info!(
                event = "peek.cli.elements_completed",
                count = result.count()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Elements listing failed: {}", e);
            error!(event = "peek.cli.elements_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_find_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let text = matches.get_one::<String>("text").unwrap();
    let json_output = matches.get_flag("json");
    let regex_flag = matches.get_flag("regex");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    info!(
        event = "peek.cli.find_started",
        text = text.as_str(),
        regex = regex_flag,
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let mut request = FindRequest::new(target, text);
    if regex_flag {
        request = request.with_regex();
    }
    if wait_flag {
        request = request.with_wait(timeout_ms);
    }

    match find_element(&request) {
        Ok(element) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&element)?);
            } else {
                println!("Found element:");
                println!("  Role: {}", element.role());
                if let Some(title) = element.title() {
                    println!("  Title: {}", title);
                }
                if let Some(value) = element.value() {
                    println!("  Value: {}", value);
                }
                if let Some(desc) = element.description() {
                    println!("  Description: {}", desc);
                }
                println!("  Position: ({}, {})", element.x(), element.y());
                println!("  Size: {}x{}", element.width(), element.height());
                println!("  Enabled: {}", element.enabled());
            }

            info!(
                event = "peek.cli.find_completed",
                text = text.as_str(),
                role = element.role()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Find failed: {}", e);
            error!(event = "peek.cli.find_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Parse click modifier from --right/--double flags and return (modifier, user-facing label)
fn parse_click_modifier(matches: &ArgMatches) -> (ClickModifier, &'static str) {
    if matches.get_flag("right") {
        (ClickModifier::Right, "Right-clicked")
    } else if matches.get_flag("double") {
        (ClickModifier::Double, "Double-clicked")
    } else {
        (ClickModifier::None, "Clicked")
    }
}

fn handle_click_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let at_str = matches.get_one::<String>("at");
    let text_str = matches.get_one::<String>("text");
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
    let wait_timeout = wait_flag.then_some(timeout_ms);
    let (modifier, label) = parse_click_modifier(matches);

    // Must have either --at or --text
    if at_str.is_none() && text_str.is_none() {
        return Err("Either --at or --text is required".into());
    }

    // Dispatch to text-based or coordinate-based click
    if let Some(text) = text_str {
        return handle_click_text(target, text, json_output, wait_timeout, modifier, label);
    }

    let at_str = at_str.unwrap();
    let (x, y) = parse_coordinates(at_str)?;

    info!(
        event = "peek.cli.interact.click_started",
        x = x,
        y = y,
        modifier = ?modifier,
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let mut request = ClickRequest::new(target, x, y).with_modifier(modifier);
    if wait_flag {
        request = request.with_wait(timeout_ms);
    }

    match click(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{} at ({}, {})", label, x, y);
                if let Some(details) = &result.details {
                    if let Some(window) = details.get("window") {
                        println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                    }
                    if let (Some(sx), Some(sy)) = (details.get("screen_x"), details.get("screen_y"))
                    {
                        println!("  Screen: ({}, {})", sx, sy);
                    }
                }
            }

            info!(event = "peek.cli.interact.click_completed", x = x, y = y, modifier = ?modifier);
            Ok(())
        }
        Err(e) => {
            eprintln!("Click failed: {}", e);
            error!(event = "peek.cli.interact.click_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_click_text(
    target: InteractionTarget,
    text: &str,
    json_output: bool,
    timeout_ms: Option<u64>,
    modifier: ClickModifier,
    label: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        event = "peek.cli.interact.click_text_started",
        text = text,
        modifier = ?modifier,
        target = ?target,
        timeout_ms = ?timeout_ms
    );

    let mut request = ClickTextRequest::new(target, text).with_modifier(modifier);
    if let Some(timeout) = timeout_ms {
        request = request.with_wait(timeout);
    }

    match click_text(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{} element with text \"{}\"", label, text);
                if let Some(details) = &result.details {
                    if let Some(window) = details.get("window") {
                        println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                    }
                    if let Some(role) = details.get("element_role") {
                        println!("  Role: {}", role.as_str().unwrap_or("unknown"));
                    }
                    if let (Some(cx), Some(cy)) = (details.get("center_x"), details.get("center_y"))
                    {
                        println!("  Center: ({}, {})", cx, cy);
                    }
                }
            }

            info!(event = "peek.cli.interact.click_text_completed", text = text, modifier = ?modifier);
            Ok(())
        }
        Err(e) => {
            eprintln!("Click by text failed: {}", e);
            error!(event = "peek.cli.interact.click_text_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_type_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let text = matches.get_one::<String>("text").unwrap();
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    info!(
        event = "peek.cli.interact.type_started",
        text_len = text.len(),
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let request = if wait_flag {
        TypeRequest::new(target, text).with_wait(timeout_ms)
    } else {
        TypeRequest::new(target, text)
    };

    match type_text(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Typed {} characters", text.len());
                if let Some(details) = &result.details
                    && let Some(window) = details.get("window")
                {
                    println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                }
            }

            info!(
                event = "peek.cli.interact.type_completed",
                text_len = text.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Type failed: {}", e);
            error!(event = "peek.cli.interact.type_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_key_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let combo = matches.get_one::<String>("combo").unwrap();
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    info!(
        event = "peek.cli.interact.key_started",
        combo = combo.as_str(),
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let request = if wait_flag {
        KeyComboRequest::new(target, combo).with_wait(timeout_ms)
    } else {
        KeyComboRequest::new(target, combo)
    };

    match send_key_combo(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Sent key: {}", combo);
                if let Some(details) = &result.details
                    && let Some(window) = details.get("window")
                {
                    println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                }
            }

            info!(
                event = "peek.cli.interact.key_completed",
                combo = combo.as_str()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Key failed: {}", e);
            error!(event = "peek.cli.interact.key_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_drag_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let from_str = matches.get_one::<String>("from").unwrap();
    let to_str = matches.get_one::<String>("to").unwrap();
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    let (from_x, from_y) = parse_coordinates(from_str)?;
    let (to_x, to_y) = parse_coordinates(to_str)?;

    info!(
        event = "peek.cli.interact.drag_started",
        from_x = from_x,
        from_y = from_y,
        to_x = to_x,
        to_y = to_y,
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let mut request = DragRequest::new(target, from_x, from_y, to_x, to_y);
    if wait_flag {
        request = request.with_wait(timeout_ms);
    }

    match drag(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!(
                    "Dragged from ({}, {}) to ({}, {})",
                    from_x, from_y, to_x, to_y
                );
                if let Some(details) = &result.details
                    && let Some(window) = details.get("window")
                {
                    println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                }
            }

            info!(
                event = "peek.cli.interact.drag_completed",
                from_x = from_x,
                from_y = from_y,
                to_x = to_x,
                to_y = to_y
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Drag failed: {}", e);
            error!(event = "peek.cli.interact.drag_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_scroll_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    // Compute deltas from direction flags
    let up = matches.get_one::<i32>("up");
    let down = matches.get_one::<i32>("down");
    let left = matches.get_one::<i32>("left");
    let scroll_right = matches.get_one::<i32>("scroll_right");

    // At least one direction is required
    if up.is_none() && down.is_none() && left.is_none() && scroll_right.is_none() {
        return Err("At least one direction is required (--up, --down, --left, --right)".into());
    }

    // delta_y: positive = scroll down, negative = scroll up
    let delta_y = down.copied().unwrap_or(0) - up.copied().unwrap_or(0);

    // delta_x: positive = scroll right, negative = scroll left
    let delta_x = scroll_right.copied().unwrap_or(0) - left.copied().unwrap_or(0);

    let at_str = matches.get_one::<String>("at");

    info!(
        event = "peek.cli.interact.scroll_started",
        delta_x = delta_x,
        delta_y = delta_y,
        at = ?at_str,
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let mut request = ScrollRequest::new(target, delta_x, delta_y);
    if let Some(at) = at_str {
        let (at_x, at_y) = parse_coordinates(at)?;
        request = request.with_at(at_x, at_y);
    }
    if wait_flag {
        request = request.with_wait(timeout_ms);
    }

    match scroll(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let mut parts = Vec::new();
                if delta_y > 0 {
                    parts.push(format!("{} lines down", delta_y));
                }
                if delta_y < 0 {
                    parts.push(format!("{} lines up", -delta_y));
                }
                if delta_x > 0 {
                    parts.push(format!("{} lines right", delta_x));
                }
                if delta_x < 0 {
                    parts.push(format!("{} lines left", -delta_x));
                }
                println!("Scrolled {}", parts.join(", "));
                if let Some(details) = &result.details
                    && let Some(window) = details.get("window")
                {
                    println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                }
            }

            info!(
                event = "peek.cli.interact.scroll_completed",
                delta_x = delta_x,
                delta_y = delta_y
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Scroll failed: {}", e);
            error!(event = "peek.cli.interact.scroll_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_hover_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let at_str = matches.get_one::<String>("at");
    let text_str = matches.get_one::<String>("text");
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
    let wait_timeout = wait_flag.then_some(timeout_ms);

    if at_str.is_none() && text_str.is_none() {
        return Err("Either --at or --text is required".into());
    }

    if let Some(text) = text_str {
        return handle_hover_text(target, text, json_output, wait_timeout);
    }

    let at_str = at_str.unwrap();
    let (x, y) = parse_coordinates(at_str)?;

    info!(
        event = "peek.cli.interact.hover_started",
        x = x,
        y = y,
        target = ?target,
        wait = wait_flag,
        timeout_ms = timeout_ms
    );

    let mut request = HoverRequest::new(target, x, y);
    if wait_flag {
        request = request.with_wait(timeout_ms);
    }

    match hover(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Hovered at ({}, {})", x, y);
                if let Some(details) = &result.details
                    && let Some(window) = details.get("window")
                {
                    println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                }
            }

            info!(event = "peek.cli.interact.hover_completed", x = x, y = y);
            Ok(())
        }
        Err(e) => {
            eprintln!("Hover failed: {}", e);
            error!(event = "peek.cli.interact.hover_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_hover_text(
    target: InteractionTarget,
    text: &str,
    json_output: bool,
    timeout_ms: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        event = "peek.cli.interact.hover_text_started",
        text = text,
        target = ?target,
        timeout_ms = ?timeout_ms
    );

    let mut request = HoverTextRequest::new(target, text);
    if let Some(timeout) = timeout_ms {
        request = request.with_wait(timeout);
    }

    match hover_text(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Hovered over element with text \"{}\"", text);
                if let Some(details) = &result.details {
                    if let Some(window) = details.get("window") {
                        println!("  Window: {}", window.as_str().unwrap_or("unknown"));
                    }
                    if let Some(role) = details.get("element_role") {
                        println!("  Role: {}", role.as_str().unwrap_or("unknown"));
                    }
                    if let (Some(cx), Some(cy)) = (details.get("center_x"), details.get("center_y"))
                    {
                        println!("  Center: ({}, {})", cx, cy);
                    }
                }
            }

            info!(
                event = "peek.cli.interact.hover_text_completed",
                text = text
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Hover by text failed: {}", e);
            error!(event = "peek.cli.interact.hover_text_failed", error = %e);
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
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);

    let threshold = (threshold_percent as f64) / 100.0;

    // Resolve the window using app and/or title, with optional wait
    let resolved_title = if wait_flag {
        resolve_window_title_with_wait(app_name, window_title, timeout_ms)?
    } else {
        resolve_window_title(app_name, window_title)?
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

/// Resolve window title from app name and/or window title
fn resolve_window_title(
    app_name: Option<&String>,
    window_title: Option<&String>,
) -> Result<String, Box<dyn std::error::Error>> {
    resolve_window_title_impl(app_name, window_title, None)
}

/// Resolve window title with wait support
fn resolve_window_title_with_wait(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    resolve_window_title_impl(app_name, window_title, Some(timeout_ms))
}

/// Implementation for window title resolution with optional wait
fn resolve_window_title_impl(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: Option<u64>,
) -> Result<String, Box<dyn std::error::Error>> {
    if app_name.is_none() && window_title.is_none() {
        return Ok(String::new());
    }

    // If only window title provided and no wait, return title directly without lookup
    if app_name.is_none()
        && timeout_ms.is_none()
        && let Some(title) = window_title
    {
        return Ok(title.clone());
    }

    // Find window based on provided parameters
    let window = find_window_with_params(app_name, window_title, timeout_ms).map_err(|e| {
        log_window_resolution_error(&e, app_name, window_title);
        format_window_resolution_error(&e, app_name, window_title)
    })?;

    Ok(window.title().to_string())
}

/// Find window with the given parameters
fn find_window_with_params(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: Option<u64>,
) -> Result<kild_peek_core::window::WindowInfo, kild_peek_core::window::WindowError> {
    match (app_name, window_title, timeout_ms) {
        (Some(app), Some(title), Some(timeout)) => {
            find_window_by_app_and_title_with_wait(app, title, timeout)
        }
        (Some(app), Some(title), None) => find_window_by_app_and_title(app, title),
        (Some(app), None, Some(timeout)) => find_window_by_app_with_wait(app, timeout),
        (Some(app), None, None) => find_window_by_app(app),
        (None, Some(title), Some(timeout)) => find_window_by_title_with_wait(title, timeout),
        (None, Some(_), None) => unreachable!("handled in early return"),
        (None, None, _) => unreachable!("handled in early return"),
    }
}

/// Log window resolution error
fn log_window_resolution_error(
    error: &kild_peek_core::window::WindowError,
    app_name: Option<&String>,
    window_title: Option<&String>,
) {
    error!(
        event = "peek.cli.assert_window_resolution_failed",
        app = ?app_name,
        title = ?window_title,
        error = %error,
        error_code = error.error_code()
    );
    events::log_app_error(error);
}

/// Format window resolution error message
fn format_window_resolution_error(
    error: &kild_peek_core::window::WindowError,
    app_name: Option<&String>,
    window_title: Option<&String>,
) -> String {
    match (app_name, window_title) {
        (Some(app), Some(title)) => {
            format!(
                "Window not found for app '{}' with title '{}': {}",
                app, title, error
            )
        }
        (Some(app), None) => format!("Window not found for app '{}': {}", app, error),
        (None, Some(title)) => format!("Window not found with title '{}': {}", title, error),
        (None, None) => format!("Window resolution error: {}", error),
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

/// Resolve a window for capture, with optional wait timeout
fn resolve_window_for_capture(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: Option<u64>,
) -> Result<kild_peek_core::window::WindowInfo, Box<dyn std::error::Error>> {
    let timeout = timeout_ms.expect("resolve_window_for_capture requires timeout");

    let window = find_window_with_wait(app_name, window_title, timeout).map_err(|e| {
        log_capture_window_resolution_error(&e, app_name, window_title);
        format_window_resolution_error(&e, app_name, window_title)
    })?;

    Ok(window)
}

/// Find window with wait based on app and/or title
fn find_window_with_wait(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout: u64,
) -> Result<kild_peek_core::window::WindowInfo, kild_peek_core::window::WindowError> {
    match (app_name, window_title) {
        (Some(app), Some(title)) => find_window_by_app_and_title_with_wait(app, title, timeout),
        (Some(app), None) => find_window_by_app_with_wait(app, timeout),
        (None, Some(title)) => find_window_by_title_with_wait(title, timeout),
        (None, None) => unreachable!("at least one of app or title must be provided"),
    }
}

/// Log window resolution error for capture operations
fn log_capture_window_resolution_error(
    error: &kild_peek_core::window::WindowError,
    app_name: Option<&String>,
    window_title: Option<&String>,
) {
    error!(
        event = "peek.cli.assert_similar_window_resolution_failed",
        app = ?app_name,
        title = ?window_title,
        error = %error,
        error_code = error.error_code()
    );
    events::log_app_error(error);
}

#[cfg(test)]
mod tests {
    // Integration tests would go here
    // Most command tests require actual windows/monitors
}
