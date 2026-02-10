use clap::ArgMatches;
use tracing::error;

use kild_core::events;

pub mod helpers;
mod json_types;

mod agent_status;
mod attach;
mod cd;
mod cleanup;
mod code;
mod commits;
mod complete;
mod create;
mod daemon;
mod destroy;
mod diff;
mod focus;
mod health;
mod hide;
mod list;
mod open;
mod overlaps;
mod pr;
mod rebase;
mod restart;
mod stats;
mod status;
mod stop;
mod sync;

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("create", sub_matches)) => create::handle_create_command(sub_matches),
        Some(("list", sub_matches)) => list::handle_list_command(sub_matches),
        Some(("cd", sub_matches)) => cd::handle_cd_command(sub_matches),
        Some(("destroy", sub_matches)) => destroy::handle_destroy_command(sub_matches),
        Some(("complete", sub_matches)) => complete::handle_complete_command(sub_matches),
        Some(("restart", sub_matches)) => restart::handle_restart_command(sub_matches),
        Some(("open", sub_matches)) => open::handle_open_command(sub_matches),
        Some(("stop", sub_matches)) => stop::handle_stop_command(sub_matches),
        Some(("code", sub_matches)) => code::handle_code_command(sub_matches),
        Some(("focus", sub_matches)) => focus::handle_focus_command(sub_matches),
        Some(("hide", sub_matches)) => hide::handle_hide_command(sub_matches),
        Some(("diff", sub_matches)) => diff::handle_diff_command(sub_matches),
        Some(("commits", sub_matches)) => commits::handle_commits_command(sub_matches),
        Some(("pr", sub_matches)) => pr::handle_pr_command(sub_matches),
        Some(("stats", sub_matches)) => stats::handle_stats_command(sub_matches),
        Some(("overlaps", sub_matches)) => overlaps::handle_overlaps_command(sub_matches),
        Some(("status", sub_matches)) => status::handle_status_command(sub_matches),
        Some(("agent-status", sub_matches)) => {
            agent_status::handle_agent_status_command(sub_matches)
        }
        Some(("rebase", sub_matches)) => rebase::handle_rebase_command(sub_matches),
        Some(("sync", sub_matches)) => sync::handle_sync_command(sub_matches),
        Some(("cleanup", sub_matches)) => cleanup::handle_cleanup_command(sub_matches),
        Some(("health", sub_matches)) => health::handle_health_command(sub_matches),
        Some(("daemon", sub_matches)) => daemon::handle_daemon_command(sub_matches),
        Some(("attach", sub_matches)) => attach::handle_attach_command(sub_matches),
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}
