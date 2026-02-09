use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

use super::helpers::{
    FailedOperation, format_partial_failure_error, is_valid_branch_name, load_config_with_warning,
};

pub(crate) fn handle_stats_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    if matches.get_flag("all") {
        let base_override = matches.get_one::<String>("base").cloned();
        let json_output = matches.get_flag("json");
        return handle_all_stats(base_override, json_output);
    }

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;
    let json_output = matches.get_flag("json");

    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.stats_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    let config = load_config_with_warning();
    let base_branch = match matches.get_one::<String>("base") {
        Some(s) => s.as_str(),
        None => config.git.base_branch(),
    };

    handle_single_stats(branch, base_branch, json_output)
}

fn handle_single_stats(
    branch: &str,
    base_branch: &str,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        event = "cli.stats_started",
        branch = branch,
        base = base_branch,
        json_output = json_output
    );

    let session = match session_ops::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.stats_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // Fetch PR info (read-only from cache)
    let pr_info = session_ops::read_pr_info(&session.id);

    let health = kild_core::git::operations::collect_branch_health(
        &session.worktree_path,
        branch,
        base_branch,
        &session.created_at,
        pr_info.as_ref(),
    );

    match health {
        Some(h) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&h)?);
            } else {
                print_single_health(branch, &h);
            }
            info!(
                event = "cli.stats_completed",
                branch = branch,
                readiness = %h.merge_readiness
            );
            Ok(())
        }
        None => {
            eprintln!("Could not compute branch health for '{}'", branch);
            error!(
                event = "cli.stats_failed",
                branch = branch,
                reason = "health_unavailable"
            );
            Err(format!("Branch health unavailable for '{}'", branch).into())
        }
    }
}

fn handle_all_stats(
    base_override: Option<String>,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.stats_all_started", base_override = ?base_override);

    let config = load_config_with_warning();
    let base_branch = match base_override.as_deref() {
        Some(base) => base,
        None => config.git.base_branch(),
    };

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        println!("No kilds found.");
        info!(event = "cli.stats_all_completed", count = 0);
        return Ok(());
    }

    let mut results: Vec<kild_core::BranchHealth> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in &sessions {
        let pr_info = session_ops::read_pr_info(&session.id);
        match kild_core::git::operations::collect_branch_health(
            &session.worktree_path,
            &session.branch,
            base_branch,
            &session.created_at,
            pr_info.as_ref(),
        ) {
            Some(h) => results.push(h),
            None => {
                errors.push((
                    session.branch.clone(),
                    "Could not compute branch health".to_string(),
                ));
            }
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        print_fleet_table(&results);
    }

    info!(
        event = "cli.stats_all_completed",
        count = results.len(),
        failed = errors.len()
    );

    if !errors.is_empty() {
        let total = results.len() + errors.len();
        return Err(format_partial_failure_error("compute stats", errors.len(), total).into());
    }

    Ok(())
}

fn print_single_health(branch: &str, h: &kild_core::BranchHealth) {
    let kild_branch = kild_core::git::operations::kild_branch_name(branch);

    println!("Branch:       {} ({})", branch, kild_branch);
    println!("Created:      {}", h.created_at);

    // Last commit + commits since base
    let last_commit = h
        .commit_activity
        .last_commit_time
        .as_deref()
        .unwrap_or("none");
    println!(
        "Last commit:  {} ({} commits since {})",
        last_commit, h.commit_activity.commits_since_base, h.drift.base_branch
    );

    // Diff vs base
    if let Some(ref diff) = h.diff_vs_base {
        println!(
            "Diff vs base: +{} -{} ({} files)",
            diff.insertions, diff.deletions, diff.files_changed
        );
    } else {
        println!("Diff vs base: unavailable");
    }

    // Position
    println!(
        "Position:     {} ahead, {} behind {}",
        h.drift.ahead, h.drift.behind, h.drift.base_branch
    );

    // Merge status
    if h.conflict_check_failed {
        println!("Merge:        Unknown (check failed)");
    } else if h.has_conflicts {
        println!("Merge:        Conflicts detected");
    } else {
        println!("Merge:        Clean (no conflicts)");
    }

    // Readiness
    let readiness_detail = match &h.merge_readiness {
        kild_core::MergeReadiness::NeedsRebase => {
            format!(
                "{} ({} behind {})",
                h.merge_readiness, h.drift.behind, h.drift.base_branch
            )
        }
        _ => h.merge_readiness.to_string(),
    };
    println!("Readiness:    {}", readiness_detail);
}

fn print_fleet_table(results: &[kild_core::BranchHealth]) {
    // Dynamic column widths
    let branch_w = results
        .iter()
        .map(|h| h.branch.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 30);
    let commits_w = 7;
    let diff_w = 12;
    let behind_w = 6;
    let conflicts_w = 9;
    let readiness_w = 18;

    // Header
    println!(
        "┌{}┬{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(branch_w + 2),
        "─".repeat(commits_w + 2),
        "─".repeat(diff_w + 2),
        "─".repeat(behind_w + 2),
        "─".repeat(conflicts_w + 2),
        "─".repeat(readiness_w + 2),
    );
    println!(
        "│ {:<branch_w$} │ {:<commits_w$} │ {:<diff_w$} │ {:<behind_w$} │ {:<conflicts_w$} │ {:<readiness_w$} │",
        "Branch", "Commits", "Diff", "Behind", "Conflicts", "Readiness",
    );
    println!(
        "├{}┼{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(branch_w + 2),
        "─".repeat(commits_w + 2),
        "─".repeat(diff_w + 2),
        "─".repeat(behind_w + 2),
        "─".repeat(conflicts_w + 2),
        "─".repeat(readiness_w + 2),
    );

    // Rows
    for h in results {
        let diff_str = h.diff_vs_base.as_ref().map_or_else(
            || "-".to_string(),
            |d| format!("+{} -{}", d.insertions, d.deletions),
        );
        let conflicts_str = if h.conflict_check_failed {
            "Unknown"
        } else if h.has_conflicts {
            "Yes"
        } else {
            "Clean"
        };

        println!(
            "│ {:<branch_w$} │ {:<commits_w$} │ {:<diff_w$} │ {:<behind_w$} │ {:<conflicts_w$} │ {:<readiness_w$} │",
            truncate_str(&h.branch, branch_w),
            h.commit_activity.commits_since_base,
            truncate_str(&diff_str, diff_w),
            h.drift.behind,
            conflicts_str,
            truncate_str(&h.merge_readiness.to_string(), readiness_w),
        );
    }

    // Footer
    println!(
        "└{}┴{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(branch_w + 2),
        "─".repeat(commits_w + 2),
        "─".repeat(diff_w + 2),
        "─".repeat(behind_w + 2),
        "─".repeat(conflicts_w + 2),
        "─".repeat(readiness_w + 2),
    );
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
