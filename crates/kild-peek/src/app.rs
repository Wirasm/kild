use clap::{Arg, ArgAction, ArgMatches, Command};

pub fn build_cli() -> Command {
    Command::new("kild-peek")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Native application inspector for AI-assisted development")
        .long_about(
            "kild-peek provides screenshot capture, window enumeration, and UI state validation \
             for native macOS applications. Designed for AI coding agents that need to see \
             and verify native UI.",
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose logging output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
        // List subcommand
        .subcommand(
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
                        ),
                )
                .subcommand(
                    Command::new("monitors").about("List all monitors").arg(
                        Arg::new("json")
                            .long("json")
                            .help("Output in JSON format")
                            .action(ArgAction::SetTrue),
                    ),
                ),
        )
        // Screenshot subcommand
        .subcommand(
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
                    Arg::new("window-id")
                        .long("window-id")
                        .help("Capture window by ID")
                        .value_parser(clap::value_parser!(u32))
                        .conflicts_with_all(["window", "monitor"]),
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
                ),
        )
        // Diff subcommand
        .subcommand(
            Command::new("diff")
                .about("Compare two images for similarity")
                .arg(
                    Arg::new("image1")
                        .help("First image path")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("image2")
                        .help("Second image path")
                        .required(true)
                        .index(2),
                )
                .arg(
                    Arg::new("threshold")
                        .long("threshold")
                        .short('t')
                        .help("Similarity threshold percentage (0-100, default: 95)")
                        .value_parser(clap::value_parser!(u8))
                        .default_value("95"),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output in JSON format")
                        .action(ArgAction::SetTrue),
                ),
        )
        // Assert subcommand
        .subcommand(
            Command::new("assert")
                .about("Run assertions on UI state (exit code indicates pass/fail)")
                .arg(
                    Arg::new("window")
                        .long("window")
                        .short('w')
                        .help("Target window by title (exact match preferred, falls back to partial)"),
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
                ),
        )
}

#[allow(dead_code)]
pub fn get_matches() -> ArgMatches {
    build_cli().get_matches()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_build() {
        let app = build_cli();
        assert_eq!(app.get_name(), "kild-peek");
    }

    #[test]
    fn test_cli_list_windows() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild-peek", "list", "windows"]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_cli_list_windows_json() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild-peek", "list", "windows", "--json"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let list_matches = matches.subcommand_matches("list").unwrap();
        let windows_matches = list_matches.subcommand_matches("windows").unwrap();
        assert!(windows_matches.get_flag("json"));
    }

    #[test]
    fn test_cli_list_monitors() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild-peek", "list", "monitors"]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_cli_screenshot_window() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild-peek", "screenshot", "--window", "Terminal"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let screenshot_matches = matches.subcommand_matches("screenshot").unwrap();
        assert_eq!(
            screenshot_matches.get_one::<String>("window").unwrap(),
            "Terminal"
        );
    }

    #[test]
    fn test_cli_screenshot_output() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "screenshot",
            "--window",
            "Terminal",
            "--output",
            "/tmp/test.png",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let screenshot_matches = matches.subcommand_matches("screenshot").unwrap();
        assert_eq!(
            screenshot_matches.get_one::<String>("output").unwrap(),
            "/tmp/test.png"
        );
    }

    #[test]
    fn test_cli_screenshot_monitor() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild-peek", "screenshot", "--monitor", "0"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let screenshot_matches = matches.subcommand_matches("screenshot").unwrap();
        assert_eq!(*screenshot_matches.get_one::<usize>("monitor").unwrap(), 0);
    }

    #[test]
    fn test_cli_screenshot_format_jpeg() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "screenshot",
            "--format",
            "jpg",
            "--quality",
            "90",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let screenshot_matches = matches.subcommand_matches("screenshot").unwrap();
        assert_eq!(
            screenshot_matches.get_one::<String>("format").unwrap(),
            "jpg"
        );
        assert_eq!(*screenshot_matches.get_one::<u8>("quality").unwrap(), 90);
    }

    #[test]
    fn test_cli_diff() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "diff",
            "/path/to/a.png",
            "/path/to/b.png",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let diff_matches = matches.subcommand_matches("diff").unwrap();
        assert_eq!(
            diff_matches.get_one::<String>("image1").unwrap(),
            "/path/to/a.png"
        );
        assert_eq!(
            diff_matches.get_one::<String>("image2").unwrap(),
            "/path/to/b.png"
        );
    }

    #[test]
    fn test_cli_diff_with_threshold() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "diff",
            "/path/to/a.png",
            "/path/to/b.png",
            "--threshold",
            "80",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let diff_matches = matches.subcommand_matches("diff").unwrap();
        assert_eq!(*diff_matches.get_one::<u8>("threshold").unwrap(), 80);
    }

    #[test]
    fn test_cli_assert_window_exists() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "assert",
            "--window",
            "Terminal",
            "--exists",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let assert_matches = matches.subcommand_matches("assert").unwrap();
        assert_eq!(
            assert_matches.get_one::<String>("window").unwrap(),
            "Terminal"
        );
        assert!(assert_matches.get_flag("exists"));
    }

    #[test]
    fn test_cli_assert_window_visible() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "assert",
            "--window",
            "Terminal",
            "--visible",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let assert_matches = matches.subcommand_matches("assert").unwrap();
        assert!(assert_matches.get_flag("visible"));
    }

    #[test]
    fn test_cli_assert_similar() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "assert",
            "--window",
            "Terminal",
            "--similar",
            "/path/to/baseline.png",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let assert_matches = matches.subcommand_matches("assert").unwrap();
        assert_eq!(
            assert_matches.get_one::<String>("similar").unwrap(),
            "/path/to/baseline.png"
        );
    }

    #[test]
    fn test_cli_verbose_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild-peek", "-v", "list", "windows"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));
    }

    #[test]
    fn test_cli_window_and_monitor_conflict() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "screenshot",
            "--window",
            "Terminal",
            "--monitor",
            "0",
        ]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_exists_and_visible_conflict() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild-peek",
            "assert",
            "--window",
            "Terminal",
            "--exists",
            "--visible",
        ]);
        assert!(matches.is_err());
    }
}
