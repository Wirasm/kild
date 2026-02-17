use clap::{Arg, ArgAction, Command};

pub fn click_subcommand() -> Command {
    Command::new("click")
        .about("Click at coordinates or on a text element within a window")
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
            Arg::new("at")
                .long("at")
                .help("Coordinates to click: x,y (relative to window top-left)")
                .conflicts_with("text"),
        )
        .arg(
            Arg::new("text")
                .long("text")
                .help("Click element by text content (uses Accessibility API)")
                .conflicts_with("at"),
        )
        .arg(
            Arg::new("right")
                .long("right")
                .help("Right-click (context menu)")
                .action(ArgAction::SetTrue)
                .conflicts_with("double"),
        )
        .arg(
            Arg::new("double")
                .long("double")
                .help("Double-click")
                .action(ArgAction::SetTrue)
                .conflicts_with("right"),
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

pub fn type_subcommand() -> Command {
    Command::new("type")
        .about("Type text into the focused element of a window")
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
                .required(true)
                .index(1)
                .help("Text to type"),
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

pub fn key_subcommand() -> Command {
    Command::new("key")
        .about("Send a key combination to a window")
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
            Arg::new("combo")
                .required(true)
                .index(1)
                .help("Key combination (e.g., \"enter\", \"tab\", \"cmd+s\", \"cmd+shift+p\")"),
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

pub fn drag_subcommand() -> Command {
    Command::new("drag")
        .about("Drag from one point to another within a window")
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
            Arg::new("from")
                .long("from")
                .required(true)
                .help("Start coordinates: x,y (relative to window top-left)"),
        )
        .arg(
            Arg::new("to")
                .long("to")
                .required(true)
                .help("End coordinates: x,y (relative to window top-left)"),
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

pub fn scroll_subcommand() -> Command {
    Command::new("scroll")
        .about("Scroll within a window")
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
            Arg::new("up")
                .long("up")
                .help("Lines to scroll up")
                .value_parser(clap::value_parser!(i32))
                .conflicts_with("down"),
        )
        .arg(
            Arg::new("down")
                .long("down")
                .help("Lines to scroll down")
                .value_parser(clap::value_parser!(i32))
                .conflicts_with("up"),
        )
        .arg(
            Arg::new("left")
                .long("left")
                .help("Lines to scroll left")
                .value_parser(clap::value_parser!(i32))
                .conflicts_with("scroll_right"),
        )
        .arg(
            Arg::new("scroll_right")
                .long("right")
                .help("Lines to scroll right")
                .value_parser(clap::value_parser!(i32))
                .conflicts_with("left"),
        )
        .arg(
            Arg::new("at")
                .long("at")
                .help("Position to scroll at: x,y (relative to window top-left)"),
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

pub fn hover_subcommand() -> Command {
    Command::new("hover")
        .about("Move the mouse to a position or element without clicking")
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
            Arg::new("at")
                .long("at")
                .help("Coordinates to hover: x,y (relative to window top-left)")
                .conflicts_with("text"),
        )
        .arg(
            Arg::new("text")
                .long("text")
                .help("Hover over element by text content (uses Accessibility API)")
                .conflicts_with("at"),
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
