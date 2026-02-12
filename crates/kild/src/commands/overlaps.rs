use clap::ArgMatches;
use tracing::{error, info};

use kild_core::session_ops;

use super::helpers::{format_partial_failure_error, load_config_with_warning, plural};

pub(crate) fn handle_overlaps_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");
    let config = load_config_with_warning();
    let base_branch = matches
        .get_one::<String>("base")
        .map(|s| s.as_str())
        .unwrap_or_else(|| config.git.base_branch());

    info!(
        event = "cli.overlaps_started",
        base = base_branch,
        json_output = json_output
    );

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        if json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "overlapping_files": [],
                    "clean_kilds": [],
                    "reason": "no_kilds_found"
                }))?
            );
        } else {
            println!("No kilds found.");
        }
        info!(event = "cli.overlaps_completed", overlap_count = 0);
        return Ok(());
    }

    if sessions.len() < 2 {
        if json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "overlapping_files": [],
                    "clean_kilds": [],
                    "reason": "insufficient_kilds"
                }))?
            );
        } else {
            println!("Only 1 kild active. Overlaps require at least 2 kilds.");
        }
        info!(event = "cli.overlaps_completed", overlap_count = 0);
        return Ok(());
    }

    let total = sessions.len();
    let (report, errors) = kild_core::git::collect_file_overlaps(&sessions, base_branch);

    info!(
        event = "cli.overlaps_completed",
        overlap_count = report.overlapping_files.len(),
        clean_count = report.clean_kilds.len(),
        errors = errors.len()
    );

    // Surface errors before the report so users see warnings first
    if !errors.is_empty() {
        let all_failed = errors.len() == total;

        if all_failed {
            eprintln!(
                "All {} {} failed to compute overlaps:",
                total,
                plural(total)
            );
        } else {
            eprintln!(
                "{} of {} {} failed (partial results):",
                errors.len(),
                total,
                plural(total)
            );
        }

        for (branch, msg) in &errors {
            eprintln!("  {} — {}", branch, msg);
        }

        if all_failed {
            error!(
                event = "cli.overlaps_failed",
                failed = errors.len(),
                total = total
            );
            return Err(format!("All {} {} failed overlap detection", total, plural(total)).into());
        }

        eprintln!();
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_overlap_report(&report);
    }

    if !errors.is_empty() {
        error!(
            event = "cli.overlaps_partial_failure",
            failed = errors.len(),
            total = total
        );
        return Err(format_partial_failure_error("compute overlaps", errors.len(), total).into());
    }

    Ok(())
}

fn print_overlap_report(report: &kild_core::OverlapReport) {
    if report.overlapping_files.is_empty() {
        println!("No file overlaps detected across kilds.");
        if !report.clean_kilds.is_empty() {
            println!();
            for clean in &report.clean_kilds {
                println!("  {} ({} files changed)", clean.branch, clean.changed_files);
            }
        }
        return;
    }

    println!("Overlapping files across kilds:");
    println!();
    for overlap in &report.overlapping_files {
        println!("  {}", overlap.file.display());
        println!("    modified by: {}", overlap.branches.join(", "));
    }

    if !report.clean_kilds.is_empty() {
        println!();
        println!("No overlaps:");
        for clean in &report.clean_kilds {
            println!(
                "  {} ({} files changed, no shared files with other kilds)",
                clean.branch, clean.changed_files
            );
        }
    }
}
