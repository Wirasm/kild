use clap::ArgMatches;
use kild_peek_core::events;
use kild_peek_core::interact::InteractionTarget;
use tracing::error;

mod assert;
mod diff;
mod elements;
mod interact;
mod list;
mod screenshot;
pub(crate) mod window_resolution;

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("list", sub_matches)) => list::handle_list_command(sub_matches),
        Some(("screenshot", sub_matches)) => screenshot::handle_screenshot_command(sub_matches),
        Some(("diff", sub_matches)) => diff::handle_diff_command(sub_matches),
        Some(("elements", sub_matches)) => elements::handle_elements_command(sub_matches),
        Some(("find", sub_matches)) => elements::handle_find_command(sub_matches),
        Some(("click", sub_matches)) => interact::handle_click_command(sub_matches),
        Some(("drag", sub_matches)) => interact::handle_drag_command(sub_matches),
        Some(("scroll", sub_matches)) => interact::handle_scroll_command(sub_matches),
        Some(("hover", sub_matches)) => interact::handle_hover_command(sub_matches),
        Some(("type", sub_matches)) => interact::handle_type_command(sub_matches),
        Some(("key", sub_matches)) => interact::handle_key_command(sub_matches),
        Some(("assert", sub_matches)) => assert::handle_assert_command(sub_matches),
        _ => {
            error!(event = "peek.cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}

/// Parse an InteractionTarget from --window and --app arguments
pub(crate) fn parse_interaction_target(
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

pub(crate) fn parse_coordinates(at_str: &str) -> Result<(i32, i32), Box<dyn std::error::Error>> {
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

#[cfg(test)]
mod tests {
    // Integration tests would go here
    // Most command tests require actual windows/monitors
}
