use clap::{Arg, ArgAction, Command};

pub fn list_command() -> Command {
    Command::new("list")
        .about("List all kild for current project")
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
}

pub fn cd_command() -> Command {
    Command::new("cd")
        .about("Print worktree path for shell integration")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild")
                .required(true)
                .index(1),
        )
}

pub fn status_command() -> Command {
    Command::new("status")
        .about("Show detailed status of a kild")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to check")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
}

pub fn agent_status_command() -> Command {
    Command::new("agent-status")
        .about("Report agent activity status (called by agent hooks)")
        .arg(
            Arg::new("target")
                .help("Branch name and status (e.g., 'mybranch working') or just status with --self (e.g., 'working')")
                .required(true)
                .num_args(1..=2)
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("self")
                .long("self")
                .help("Auto-detect session from current working directory")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("notify")
                .long("notify")
                .help("Send desktop notification when status is 'waiting' or 'error'")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
}
