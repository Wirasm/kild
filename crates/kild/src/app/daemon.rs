use clap::{Arg, ArgAction, Command};

pub fn daemon_command() -> Command {
    Command::new("daemon")
        .about("Manage the KILD daemon")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("start")
                .about("Start the KILD daemon in the background")
                .arg(
                    Arg::new("foreground")
                        .long("foreground")
                        .help("Run daemon in the foreground (for debugging)")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(Command::new("stop").about("Stop the running KILD daemon"))
        .subcommand(
            Command::new("status").about("Show daemon status").arg(
                Arg::new("json")
                    .long("json")
                    .help("Output as JSON")
                    .action(ArgAction::SetTrue),
            ),
        )
}

pub fn attach_command() -> Command {
    Command::new("attach")
        .about("Attach to a daemon-managed kild session")
        .arg(
            Arg::new("branch")
                .help("Branch name of the kild to attach to")
                .required(true)
                .index(1),
        )
}
