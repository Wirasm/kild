use clap::{Arg, ArgAction, Command};

pub fn subcommand() -> Command {
    Command::new("list")
        .about("List windows or monitors")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("windows")
                .about("List all visible windows")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output in JSON format")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("app")
                        .long("app")
                        .short('a')
                        .help("Filter windows by app name"),
                ),
        )
        .subcommand(
            Command::new("monitors").about("List all monitors").arg(
                Arg::new("json")
                    .long("json")
                    .help("Output in JSON format")
                    .action(ArgAction::SetTrue),
            ),
        )
}
