mod daemon;
mod git;
mod global;
mod misc;
mod query;
mod session;

#[cfg(test)]
mod tests;

use clap::Command;

pub fn build_cli() -> Command {
    global::root_command()
        .subcommand(session::create_command())
        .subcommand(query::list_command())
        .subcommand(query::cd_command())
        .subcommand(session::destroy_command())
        .subcommand(session::complete_command())
        .subcommand(session::open_command())
        .subcommand(session::stop_command())
        .subcommand(misc::code_command())
        .subcommand(misc::focus_command())
        .subcommand(misc::hide_command())
        .subcommand(git::diff_command())
        .subcommand(git::commits_command())
        .subcommand(misc::pr_command())
        .subcommand(query::status_command())
        .subcommand(query::agent_status_command())
        .subcommand(git::rebase_command())
        .subcommand(git::sync_command())
        .subcommand(misc::cleanup_command())
        .subcommand(misc::stats_command())
        .subcommand(misc::overlaps_command())
        .subcommand(misc::health_command())
        .subcommand(daemon::daemon_command())
        .subcommand(daemon::attach_command())
        .subcommand(misc::completions_command())
        .subcommand(misc::init_hooks_command())
}
