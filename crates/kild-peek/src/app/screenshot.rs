use clap::{Arg, ArgAction, Command};

pub fn subcommand() -> Command {
    Command::new("screenshot")
        .about("Capture a screenshot")
        .arg(
            Arg::new("window")
                .long("window")
                .short('w')
                .help("Capture window by title (exact match preferred, falls back to partial)")
                .conflicts_with_all(["window-id", "monitor"]),
        )
        .arg(
            Arg::new("app")
                .long("app")
                .short('a')
                .help("Capture window by app name (can combine with --window for precision)")
                .conflicts_with_all(["window-id", "monitor"]),
        )
        .arg(
            Arg::new("window-id")
                .long("window-id")
                .help("Capture window by ID")
                .value_parser(clap::value_parser!(u32))
                .conflicts_with_all(["window", "app", "monitor"]),
        )
        .arg(
            Arg::new("monitor")
                .long("monitor")
                .short('m')
                .help("Capture specific monitor by index (default: primary)")
                .value_parser(clap::value_parser!(usize)),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("Save to file path (default: output base64 to stdout)"),
        )
        .arg(
            Arg::new("base64")
                .long("base64")
                .help("Output base64 encoded image (default if no --output)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .short('f')
                .help("Output format")
                .value_parser(["png", "jpg", "jpeg"])
                .default_value("png"),
        )
        .arg(
            Arg::new("quality")
                .long("quality")
                .help("JPEG quality (1-100, default: 85)")
                .value_parser(clap::value_parser!(u8))
                .default_value("85"),
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
        .arg(
            Arg::new("crop")
                .long("crop")
                .help("Crop to region: x,y,width,height (e.g., \"0,0,400,50\")")
                .value_name("REGION"),
        )
}
