use clap::ArgMatches;
use kild_peek_core::events;
use kild_peek_core::interact::{
    ClickModifier, ClickRequest, ClickTextRequest, DragRequest, HoverRequest, HoverTextRequest,
    InteractionTarget, KeyComboRequest, ScrollRequest, TypeRequest, click, click_text, drag, hover,
    hover_text, scroll, send_key_combo, type_text,
};
use tracing::{error, info};

use super::{parse_coordinates, parse_interaction_target};

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

pub fn handle_click_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn handle_type_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn handle_key_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn handle_drag_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn handle_scroll_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn handle_hover_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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
