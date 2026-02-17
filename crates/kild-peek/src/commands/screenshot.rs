use std::path::PathBuf;

use clap::ArgMatches;
use kild_peek_core::events;
use kild_peek_core::screenshot::{CaptureRequest, CropArea, ImageFormat, capture, save_to_file};
use tracing::{error, info, warn};

use super::window_resolution::resolve_window_for_capture;

pub fn handle_screenshot_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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
