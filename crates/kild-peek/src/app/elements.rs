use clap::{Arg, ArgAction, Command};

pub fn subcommand() -> Command {
    Command::new("elements")
        .about("List all UI elements in a window via Accessibility API")
        .arg(
            Arg::new("window")
                .long("window")
                .short('w')
                .help("Target window by title"),
        )
        .arg(
            Arg::new("app")
                .long("app")
                .short('a')
                .help("Target window by app name"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tree")
                .long("tree")
                .help("Display elements as indented tree hierarchy")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("wait")
                .long("wait")
                .help("Wait for window to appear (polls until found or timeout)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .help("Timeout in milliseconds when using --wait (default: 30000)")
                .value_parser(clap::value_parser!(u64))
                .default_value("30000"),
        )
}

pub fn wait_subcommand() -> Command {
    Command::new("wait")
        .about("Wait for a UI element with given text to appear or disappear")
        .arg(
            Arg::new("window")
                .long("window")
                .short('w')
                .help("Target window by title"),
        )
        .arg(
            Arg::new("app")
                .long("app")
                .short('a')
                .help("Target window by app name"),
        )
        .arg(
            Arg::new("text")
                .long("text")
                .required(true)
                .help("Text to wait for (case-insensitive substring match)"),
        )
        .arg(
            Arg::new("until-gone")
                .long("until-gone")
                .help("Wait for element to disappear instead of appear")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .help("Timeout in milliseconds (default: 30000)")
                .value_parser(clap::value_parser!(u64))
                .default_value("30000"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output result in JSON format")
                .action(ArgAction::SetTrue),
        )
}

pub fn find_subcommand() -> Command {
    Command::new("find")
        .about("Find a UI element by text content")
        .arg(
            Arg::new("window")
                .long("window")
                .short('w')
                .help("Target window by title"),
        )
        .arg(
            Arg::new("app")
                .long("app")
                .short('a')
                .help("Target window by app name"),
        )
        .arg(
            Arg::new("text")
                .long("text")
                .required(true)
                .help("Text to search for in element title, value, or description"),
        )
        .arg(
            Arg::new("regex")
                .long("regex")
                .help("Treat --text value as a regex pattern")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("wait")
                .long("wait")
                .help("Wait for window to appear (polls until found or timeout)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .help("Timeout in milliseconds when using --wait (default: 30000)")
                .value_parser(clap::value_parser!(u64))
                .default_value("30000"),
        )
}
