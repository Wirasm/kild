use clap::ArgMatches;
use serde::Serialize;
use tracing::{error, info};

use kild_core::BranchHealth;
use kild_core::ConflictStatus;
use kild_core::MergeReadiness;
use kild_core::session_ops;

use super::helpers::{
    self, FailedOperation, format_partial_failure_error, is_valid_branch_name,
    load_config_with_warning,
};

/// Combined output for JSON: git health + computed readiness.
#[derive(Serialize)]
struct StatsOutput {
    #[serde(flatten)]
    health: BranchHealth,
    merge_readiness: MergeReadiness,
}

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
        if json_output {
            let err_msg = format!("Invalid branch name: {}", branch);
            let boxed = super::helpers::print_json_error(&err_msg, "INVALID_BRANCH_NAME");
            error!(event = "cli.stats_invalid_branch", branch = branch);
            return Err(boxed);
        }
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

    let session = helpers::require_session_json(branch, "cli.stats_failed", json_output)?;

    if session.use_main_worktree {
        let msg = format!(
            "Branch health is not available for --main sessions (no isolated worktree). \
             '{}' runs from the project root.",
            branch
        );
        if json_output {
            return Err(super::helpers::print_json_error(
                &msg,
                "MAIN_WORKTREE_SESSION",
            ));
        }
        eprintln!("{}", msg);
        return Err(msg.into());
    }

    let health = kild_core::git::collect_branch_health(
        &session.worktree_path,
        branch,
        base_branch,
        &session.created_at,
    );

    match health {
        Ok(h) => {
            // Compose: git health + worktree status + PR info → readiness
            let worktree_status = kild_core::git::get_worktree_status(&session.worktree_path).ok();
            let pr_info = session_ops::read_pr_info(&session.id);
            let readiness = MergeReadiness::compute(&h, &worktree_status, pr_info.as_ref());

            info!(
                event = "cli.stats_completed",
                branch = branch,
                readiness = %readiness
            );

            if json_output {
                let output = StatsOutput {
                    health: h,
                    merge_readiness: readiness,
                };
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                print_single_health(branch, &h, &readiness);
            }
            Ok(())
        }
        Err(msg) => {
            error!(
                event = "cli.stats_failed",
                branch = branch,
                reason = "health_unavailable"
            );

            if json_output {
                let err_msg = format!("Could not compute branch health for '{}': {}", branch, msg);
                return Err(super::helpers::print_json_error(
                    &err_msg,
                    "HEALTH_UNAVAILABLE",
                ));
            }
            eprintln!("Could not compute branch health for '{}': {}", branch, msg);
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
        if json_output {
            println!("[]");
        } else {
            println!("No kilds found.");
        }
        info!(event = "cli.stats_all_completed", count = 0);
        return Ok(());
    }

    let mut results: Vec<(BranchHealth, MergeReadiness)> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in &sessions {
        // Skip --main sessions — they have no isolated branch to measure.
        if session.use_main_worktree {
            info!(
                event = "cli.stats_skip_main_session",
                branch = %session.branch,
            );
            continue;
        }

        match kild_core::git::collect_branch_health(
            &session.worktree_path,
            &session.branch,
            base_branch,
            &session.created_at,
        ) {
            Ok(h) => {
                let worktree_status =
                    kild_core::git::get_worktree_status(&session.worktree_path).ok();
                let pr_info = session_ops::read_pr_info(&session.id);
                let readiness = MergeReadiness::compute(&h, &worktree_status, pr_info.as_ref());
                results.push((h, readiness));
            }
            Err(msg) => {
                errors.push((session.branch.to_string(), msg));
            }
        }
    }

    let result_count = results.len();

    info!(
        event = "cli.stats_all_completed",
        count = result_count,
        failed = errors.len()
    );

    if json_output {
        let output: Vec<StatsOutput> = results
            .into_iter()
            .map(|(health, merge_readiness)| StatsOutput {
                health,
                merge_readiness,
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_fleet_table(&results);
    }

    if !errors.is_empty() {
        eprintln!();
        for (branch, msg) in &errors {
            eprintln!(
                "{} '{}': {}",
                crate::color::error("Stats failed for"),
                branch,
                msg
            );
        }
        let total = result_count + errors.len();
        return Err(format_partial_failure_error("compute stats", errors.len(), total).into());
    }

    Ok(())
}

fn print_single_health(branch: &str, h: &BranchHealth, readiness: &MergeReadiness) {
    let kild_branch = kild_core::git::kild_branch_name(branch);

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
    let diff_str = match &h.diff_vs_base {
        Some(diff) => format!(
            "+{} -{} ({} files)",
            diff.insertions, diff.deletions, diff.files_changed
        ),
        None => "unavailable".to_string(),
    };
    println!("Diff vs base: {}", diff_str);

    // Position
    println!(
        "Position:     {} ahead, {} behind {}",
        h.drift.ahead, h.drift.behind, h.drift.base_branch
    );

    // Merge status
    let merge_str = match h.conflict_status {
        ConflictStatus::Clean => "Clean (no conflicts)",
        ConflictStatus::Conflicts => "Conflicts detected",
        ConflictStatus::Unknown => "Unknown (check failed)",
    };
    println!("Merge:        {}", merge_str);

    // Readiness
    let readiness_detail = match readiness {
        MergeReadiness::NeedsRebase => {
            format!(
                "{} ({} behind {})",
                readiness, h.drift.behind, h.drift.base_branch
            )
        }
        _ => readiness.to_string(),
    };
    println!("Readiness:    {}", readiness_detail);
}

fn print_fleet_table(results: &[(BranchHealth, MergeReadiness)]) {
    // Dynamic column widths
    let branch_w = results
        .iter()
        .map(|(h, _)| h.branch.len())
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
    for (h, readiness) in results {
        let diff_str = h.diff_vs_base.as_ref().map_or_else(
            || "-".to_string(),
            |d| format!("+{} -{}", d.insertions, d.deletions),
        );
        let conflicts_str = match h.conflict_status {
            ConflictStatus::Clean => "Clean",
            ConflictStatus::Conflicts => "Yes",
            ConflictStatus::Unknown => "Unknown",
        };

        println!(
            "│ {:<branch_w$} │ {:<commits_w$} │ {:<diff_w$} │ {:<behind_w$} │ {:<conflicts_w$} │ {:<readiness_w$} │",
            truncate_str(&h.branch, branch_w),
            h.commit_activity.commits_since_base,
            truncate_str(&diff_str, diff_w),
            h.drift.behind,
            conflicts_str,
            truncate_str(&readiness.to_string(), readiness_w),
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
        return s.to_string();
    }

    let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
    format!("{}...", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str_exact_length() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_shorter() {
        assert_eq!(truncate_str("hi", 5), "hi");
    }

    #[test]
    fn test_truncate_str_longer() {
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_str_zero_max() {
        assert_eq!(truncate_str("hello", 0), "...");
    }
}
