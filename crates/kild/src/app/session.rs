use clap::{Arg, ArgAction, Command};

pub fn create_command() -> Command {
    Command::new("create")
        .about("Create a new kild with git worktree and launch agent")
        .arg(
            Arg::new("branch")
                .help("Branch name for the kild")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .short('a')
                .help("AI agent to launch (overrides config)")
                .value_parser(["amp", "claude", "kiro", "gemini", "codex", "opencode"]),
        )
        .arg(
            Arg::new("terminal")
                .long("terminal")
                .short('t')
                .help("Terminal to use (overrides config)"),
        )
        .arg(
            Arg::new("startup-command")
                .long("startup-command")
                .help("Agent startup command (overrides config)"),
        )
        .arg(
            Arg::new("flags")
                .long("flags")
                .num_args(1)
                .allow_hyphen_values(true)
                .help("Additional flags for agent (use --flags 'value' or --flags='value')"),
        )
        .arg(
            Arg::new("note")
                .long("note")
                .short('n')
                .help("Description of what this kild is for (shown in list/status output)"),
        )
        .arg(
            Arg::new("base")
                .long("base")
                .short('b')
                .help("Base branch to create worktree from (overrides config, default: main)"),
        )
        .arg(
            Arg::new("no-fetch")
                .long("no-fetch")
                .help("Skip fetching from remote before creating worktree")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("yolo")
                .long("yolo")
                .help("Enable full autonomy mode (skip all permission prompts)")
                .action(ArgAction::SetTrue)
                .conflicts_with("no-agent"),
        )
        .arg(
            Arg::new("no-agent")
                .long("no-agent")
                .help("Create with a bare terminal shell instead of launching an agent")
                .action(ArgAction::SetTrue)
                .conflicts_with("agent")
                .conflicts_with("startup-command")
                .conflicts_with("flags"),
        )
        .arg(
            Arg::new("daemon")
                .long("daemon")
                .help("Launch agent in daemon-owned PTY (overrides config)")
                .action(ArgAction::SetTrue)
                .conflicts_with("no-daemon"),
        )
        .arg(
            Arg::new("no-daemon")
                .long("no-daemon")
                .help("Launch agent in external terminal window (overrides config)")
                .action(ArgAction::SetTrue)
                .conflicts_with("daemon"),
        )
}

pub fn open_command() -> Command {
    Command::new("open")
        .about("Open a new agent terminal in an existing kild (additive)")
        .arg(
            Arg::new("branch")
                .help("Branch name or kild identifier")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .short('a')
                .help("Agent to launch (default: kild's original agent)")
                .value_parser(["amp", "claude", "kiro", "gemini", "codex", "opencode"]),
        )
        .arg(
            Arg::new("no-agent")
                .long("no-agent")
                .help("Open a bare terminal with default shell instead of an agent")
                .action(ArgAction::SetTrue)
                .conflicts_with("agent"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Open agents in all stopped kild")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
        .arg(
            Arg::new("resume")
                .long("resume")
                .short('r')
                .help("Resume the previous agent conversation instead of starting fresh")
                .action(ArgAction::SetTrue)
                .conflicts_with("no-agent"),
        )
        .arg(
            Arg::new("yolo")
                .long("yolo")
                .help("Enable full autonomy mode (skip all permission prompts)")
                .action(ArgAction::SetTrue)
                .conflicts_with("no-agent"),
        )
        .arg(
            Arg::new("daemon")
                .long("daemon")
                .help("Launch agent in daemon-owned PTY (overrides config)")
                .action(ArgAction::SetTrue)
                .conflicts_with("no-daemon"),
        )
        .arg(
            Arg::new("no-daemon")
                .long("no-daemon")
                .help("Launch agent in external terminal window (overrides config)")
                .action(ArgAction::SetTrue)
                .conflicts_with("daemon"),
        )
}

pub fn stop_command() -> Command {
    Command::new("stop")
        .about("Stop agent(s) in a kild without destroying the worktree")
        .arg(
            Arg::new("branch")
                .help("Branch name or kild identifier")
                .index(1)
                .required_unless_present("all"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Stop all running kild")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
}

pub fn destroy_command() -> Command {
    Command::new("destroy")
        .about("Remove kild completely")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to destroy")
                .required_unless_present("all")
                .index(1),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .short('f')
                .help("Force destroy, bypassing git uncommitted changes check and confirmation prompt")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Destroy all kild for current project")
                .action(ArgAction::SetTrue)
                .conflicts_with("branch"),
        )
}

pub fn complete_command() -> Command {
    Command::new("complete")
        .about("Complete a kild: destroy and clean up remote branch if PR was merged")
        .long_about(
            "Completes a kild by destroying the worktree and optionally deleting the remote branch.\n\n\
            If the PR was already merged (user ran 'gh pr merge' first), this command also deletes\n\
            the orphaned remote branch. If the PR hasn't been merged yet, it just destroys the kild\n\
            so that 'gh pr merge --delete-branch' can work afterwards.\n\n\
            Works with either workflow:\n\
            - Complete first, then merge: kild complete → gh pr merge --delete-branch\n\
            - Merge first, then complete: gh pr merge → kild complete (deletes remote)"
        )
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to complete")
                .required(true)
                .index(1),
        )
}
