use clap::{Arg, ArgAction, Command};

pub fn subcommand() -> Command {
    Command::new("assert")
        .about("Run assertions on UI state (exit code indicates pass/fail)")
        .arg(
            Arg::new("window")
                .long("window")
                .short('w')
                .help("Target window by title (exact match preferred, falls back to partial)"),
        )
        .arg(
            Arg::new("app")
                .long("app")
                .short('a')
                .help("Target window by app name (can combine with --window for precision)"),
        )
        .arg(
            Arg::new("exists")
                .long("exists")
                .help("Assert window exists")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["visible", "similar"]),
        )
        .arg(
            Arg::new("visible")
                .long("visible")
                .help("Assert window is visible (not minimized)")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["exists", "similar"]),
        )
        .arg(
            Arg::new("similar")
                .long("similar")
                .help("Assert screenshot is similar to baseline image path")
                .conflicts_with_all(["exists", "visible"]),
        )
        .arg(
            Arg::new("threshold")
                .long("threshold")
                .short('t')
                .help("Similarity threshold for --similar (0-100, default: 95)")
                .value_parser(clap::value_parser!(u8))
                .default_value("95"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output assertion result in JSON format")
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
