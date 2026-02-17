use clap::{Arg, ArgAction, Command};

pub fn diff_command() -> Command {
    Command::new("diff")
        .about("Show git diff for a kild's worktree")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("staged")
                .long("staged")
                .help("Show only staged changes (git diff --staged)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("stat")
                .long("stat")
                .help("Show unstaged diffstat summary instead of full diff")
                .action(ArgAction::SetTrue),
        )
}

pub fn commits_command() -> Command {
    Command::new("commits")
        .about("Show recent commits in a kild's branch")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("count")
                .long("count")
                .short('n')
                .help("Number of commits to show (default: 10)")
                .value_parser(clap::value_parser!(usize))
                .default_value("10"),
        )
}

pub fn rebase_command() -> Command {
    Command::new("rebase")
        .about("Rebase a kild's branch onto the base branch")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to rebase")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("base")
                .long("base")
                .short('b')
                .help("Base branch to rebase onto (overrides config, default: main)"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Rebase all active kilds")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
}

pub fn sync_command() -> Command {
    Command::new("sync")
        .about("Fetch from remote and rebase a kild's branch onto the base branch")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to sync")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("base")
                .long("base")
                .short('b')
                .help("Base branch to rebase onto (overrides config, default: main)"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Fetch and rebase all active kilds")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
}
