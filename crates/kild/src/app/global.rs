use clap::{Arg, ArgAction, Command};

pub fn root_command() -> Command {
    Command::new("kild")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage parallel AI development agents in isolated Git worktrees")
        .long_about("KILD creates isolated git worktrees and launches AI coding agents in dedicated terminal windows. Each 'kild' is a disposable work context where an AI agent can operate autonomously without disrupting your main working directory.")
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose logging output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .help("Disable colored output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
}
