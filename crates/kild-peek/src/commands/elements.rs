use clap::ArgMatches;
use kild_peek_core::element::{
    ElementsRequest, FindRequest, WaitRequest, find_element, list_elements, wait_for_element,
};
use kild_peek_core::events;
use tracing::{error, info};

use crate::table;

use super::parse_interaction_target;

pub fn handle_elements_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn handle_wait_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let text = matches
        .get_one::<String>("text")
        .ok_or("--text is required")?;
    let until_gone = matches.get_flag("until-gone");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
    let json_output = matches.get_flag("json");

    info!(
        event = "peek.cli.wait_started",
        text = text.as_str(),
        until_gone = until_gone,
        timeout_ms = timeout_ms
    );

    let mut request = WaitRequest::new(target, text, timeout_ms);
    if until_gone {
        request = request.with_until_gone();
    }

    match wait_for_element(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if until_gone {
                println!("Element \"{}\" is gone ({}ms)", text, result.elapsed_ms());
            } else {
                println!("Element \"{}\" appeared ({}ms)", text, result.elapsed_ms());
            }
            info!(
                event = "peek.cli.wait_completed",
                text = text.as_str(),
                elapsed_ms = result.elapsed_ms()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Wait failed: {}", e);
            error!(event = "peek.cli.wait_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

pub fn handle_find_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
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
