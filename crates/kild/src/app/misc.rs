use clap::{Arg, ArgAction, Command};
use clap_complete::Shell;

pub fn cleanup_command() -> Command {
    Command::new("cleanup")
        .about("Clean up orphaned resources (branches, worktrees, sessions)")
        .arg(
            Arg::new("no-pid")
                .long("no-pid")
                .help("Clean only sessions without PID tracking")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("stopped")
                .long("stopped")
                .help("Clean only sessions with stopped processes")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("older-than")
                .long("older-than")
                .help("Clean sessions older than N days (e.g., 7)")
                .value_name("DAYS")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Clean all orphaned resources (default)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("orphans")
                .long("orphans")
                .help("Clean worktrees in kild directory that have no session")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .short('f')
                .help("Remove orphaned worktrees even if they have uncommitted changes or active processes")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["no-pid", "stopped", "older-than"]),
        )
}

pub fn stats_command() -> Command {
    Command::new("stats")
        .about("Show branch health and merge readiness for a kild")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Show stats for all kilds")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
        .arg(
            Arg::new("base")
                .long("base")
                .short('b')
                .help("Base branch to compare against (overrides config, default: main)"),
        )
}

pub fn inbox_command() -> Command {
    Command::new("inbox")
        .about("Inspect fleet dropbox protocol state for a kild")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Show inbox state for all fleet kilds")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
        .arg(
            Arg::new("task")
                .long("task")
                .help("Show only the current task content")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["report", "status", "all", "json"]),
        )
        .arg(
            Arg::new("report")
                .long("report")
                .help("Show only the latest report")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["task", "status", "all", "json"]),
        )
        .arg(
            Arg::new("status")
                .long("status")
                .help("Show only task-id vs ack status")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["task", "report", "all", "json"]),
        )
}

pub fn overlaps_command() -> Command {
    Command::new("overlaps")
        .about("Detect file overlaps across kilds in the current project")
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("base")
                .long("base")
                .short('b')
                .help("Base branch to compare against (overrides config, default: main)"),
        )
}

pub fn health_command() -> Command {
    Command::new("health")
        .about("Show health status and metrics for kild")
        .arg(
            Arg::new("branch")
                .help("Branch name of specific kild to check (optional)")
                .index(1),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("watch")
                .long("watch")
                .short('w')
                .help("Continuously refresh health display")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("interval")
                .long("interval")
                .short('i')
                .help("Refresh interval in seconds (default: 5)")
                .value_parser(clap::value_parser!(u64))
                .default_value("5"),
        )
}

pub fn completions_command() -> Command {
    Command::new("completions")
        .about("Generate shell completion scripts")
        .arg(
            Arg::new("shell")
                .help("Target shell")
                .required(true)
                .index(1)
                .value_parser(clap::value_parser!(Shell)),
        )
}

pub fn init_hooks_command() -> Command {
    Command::new("init-hooks")
        .about("Initialize agent integration hooks in the current project")
        .arg(
            Arg::new("agent")
                .help("Agent to configure (claude, opencode)")
                .required(true)
                .index(1)
                .value_parser(["claude", "opencode"]),
        )
        .arg(
            Arg::new("no-install")
                .long("no-install")
                .help("Skip running bun install after generating files")
                .action(ArgAction::SetTrue),
        )
}

pub fn code_command() -> Command {
    Command::new("code")
        .about("Open kild's worktree in your code editor")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to open")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("editor")
                .long("editor")
                .short('e')
                .help("Editor to use (overrides config, $EDITOR, and default 'zed')"),
        )
}

pub fn focus_command() -> Command {
    Command::new("focus")
        .about("Bring a kild's terminal window to the foreground")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to focus")
                .required(true)
                .index(1),
        )
}

pub fn hide_command() -> Command {
    Command::new("hide")
        .about("Minimize/hide a kild's terminal window")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to hide")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Hide all active kild terminal windows")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
}

pub fn pr_command() -> Command {
    Command::new("pr")
        .about("Show PR status for a kild")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("refresh")
                .long("refresh")
                .help("Force refresh PR data from GitHub")
                .action(ArgAction::SetTrue),
        )
}
