use clap::{Arg, ArgAction, ArgMatches, Command};

pub fn build_cli() -> Command {
    Command::new("shards")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage parallel AI development agents in isolated Git worktrees")
        .long_about("Shards creates isolated git worktrees and launches AI coding agents in dedicated terminal windows. Each 'shard' is a disposable work context where an AI agent can operate autonomously without disrupting your main working directory.")
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Suppress log output, show only essential information")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("create")
                .about("Create a new shard with git worktree and launch agent")
                .arg(
                    Arg::new("branch")
                        .help("Branch name for the shard")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("agent")
                        .long("agent")
                        .short('a')
                        .help("AI agent to launch (overrides config)")
                        .value_parser(["claude", "kiro", "gemini", "codex", "aether"])
                )
                .arg(
                    Arg::new("terminal")
                        .long("terminal")
                        .short('t')
                        .help("Terminal to use (overrides config)")
                )
                .arg(
                    Arg::new("startup-command")
                        .long("startup-command")
                        .help("Agent startup command (overrides config)")
                )
                .arg(
                    Arg::new("flags")
                        .long("flags")
                        .num_args(1)
                        .allow_hyphen_values(true) // Allow flag values starting with hyphens (e.g., --trust-all-tools)
                        .help("Additional flags for agent (use --flags 'value' or --flags='value')")
                )
                .arg(
                    Arg::new("note")
                        .long("note")
                        .short('n')
                        .help("Description of what this shard is for (shown in list/status output)")
                )
        )
        .subcommand(
            Command::new("list")
                .about("List all shards for current project")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output in JSON format")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("cd")
                .about("Print worktree path for shell integration")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard")
                        .required(true)
                        .index(1)
                )
        )
        .subcommand(
            Command::new("destroy")
                .about("Remove shard completely")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard to destroy")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .short('f')
                        .help("Force destroy, bypassing git uncommitted changes check")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("open")
                .about("Open a new agent terminal in an existing shard (additive)")
                .arg(
                    Arg::new("branch")
                        .help("Branch name or shard identifier")
                        .index(1)
                        .required_unless_present("all")
                )
                .arg(
                    Arg::new("agent")
                        .long("agent")
                        .short('a')
                        .help("Agent to launch (default: shard's original agent)")
                        .value_parser(["claude", "kiro", "gemini", "codex", "aether"])
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Open agents in all stopped shards")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
                )
        )
        .subcommand(
            Command::new("stop")
                .about("Stop agent(s) in a shard without destroying the worktree")
                .arg(
                    Arg::new("branch")
                        .help("Branch name or shard identifier")
                        .index(1)
                        .required_unless_present("all")
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Stop all running shards")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
                )
        )
        .subcommand(
            Command::new("code")
                .about("Open shard's worktree in your code editor")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard to open")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("editor")
                        .long("editor")
                        .short('e')
                        .help("Editor to use (defaults to $EDITOR or 'zed')")
                )
        )
        .subcommand(
            Command::new("focus")
                .about("Bring a shard's terminal window to the foreground")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard to focus")
                        .required(true)
                        .index(1)
                )
        )
        .subcommand(
            Command::new("diff")
                .about("Show git diff for a shard's worktree")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("staged")
                        .long("staged")
                        .help("Show only staged changes (git diff --staged)")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("commits")
                .about("Show recent commits in a shard's branch")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("count")
                        .long("count")
                        .short('n')
                        .help("Number of commits to show (default: 10)")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("10")
                )
        )
        .subcommand(
            Command::new("restart")
                .about("Restart agent in existing shard without destroying worktree")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard to restart")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("agent")
                        .long("agent")
                        .short('a')
                        .help("AI agent to use (overrides current agent)")
                        .value_parser(["claude", "kiro", "gemini", "codex", "aether"])
                )
        )
        .subcommand(
            Command::new("status")
                .about("Show detailed status of a shard")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the shard to check")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output in JSON format")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("cleanup")
                .about("Clean up orphaned resources (branches, worktrees, sessions)")
                .arg(
                    Arg::new("no-pid")
                        .long("no-pid")
                        .help("Clean only sessions without PID tracking")
                        .action(ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("stopped")
                        .long("stopped")
                        .help("Clean only sessions with stopped processes")
                        .action(ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("older-than")
                        .long("older-than")
                        .help("Clean sessions older than N days (e.g., 7)")
                        .value_name("DAYS")
                        .value_parser(clap::value_parser!(u64))
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Clean all orphaned resources (default)")
                        .action(ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("orphans")
                        .long("orphans")
                        .help("Clean worktrees in shards directory that have no session")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("health")
                .about("Show health status and metrics for shards")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of specific shard to check (optional)")
                        .index(1)
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output in JSON format")
                        .action(clap::ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("watch")
                        .long("watch")
                        .short('w')
                        .help("Continuously refresh health display")
                        .action(clap::ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("interval")
                        .long("interval")
                        .short('i')
                        .help("Refresh interval in seconds (default: 5)")
                        .value_parser(clap::value_parser!(u64))
                        .default_value("5")
                )
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
        assert_eq!(app.get_name(), "shards");
    }

    #[test]
    fn test_cli_create_command() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["shards", "create", "test-branch", "--agent", "kiro"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert_eq!(create_matches.get_one::<String>("agent").unwrap(), "kiro");
    }

    #[test]
    fn test_cli_list_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.subcommand_matches("list").is_some());
    }

    #[test]
    fn test_cli_list_json_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "list", "--json"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let list_matches = matches.subcommand_matches("list").unwrap();
        assert!(list_matches.get_flag("json"));
    }

    #[test]
    fn test_cli_status_json_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "status", "test-branch", "--json"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let status_matches = matches.subcommand_matches("status").unwrap();
        assert_eq!(
            status_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert!(status_matches.get_flag("json"));
    }

    #[test]
    fn test_cli_destroy_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "destroy", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let destroy_matches = matches.subcommand_matches("destroy").unwrap();
        assert_eq!(
            destroy_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_default_agent() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "create", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        // Agent is now optional, should be None when not specified
        assert!(create_matches.get_one::<String>("agent").is_none());
    }

    #[test]
    fn test_cli_invalid_agent() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "shards",
            "create",
            "test-branch",
            "--agent",
            "invalid",
        ]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_create_with_complex_flags() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "shards",
            "create",
            "test-branch",
            "--agent",
            "kiro",
            "--flags",
            "--trust-all-tools --verbose --debug",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("flags").unwrap(),
            "--trust-all-tools --verbose --debug"
        );
    }

    #[test]
    fn test_cli_health_watch_mode() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["shards", "health", "--watch", "--interval", "10"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let health_matches = matches.subcommand_matches("health").unwrap();
        assert!(health_matches.get_flag("watch"));
        assert_eq!(*health_matches.get_one::<u64>("interval").unwrap(), 10);
    }

    #[test]
    fn test_cli_health_default_interval() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "health", "--watch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let health_matches = matches.subcommand_matches("health").unwrap();
        assert!(health_matches.get_flag("watch"));
        assert_eq!(*health_matches.get_one::<u64>("interval").unwrap(), 5);
    }

    #[test]
    fn test_cli_create_with_note() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "shards",
            "create",
            "feature-branch",
            "--note",
            "This is a test note",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("branch").unwrap(),
            "feature-branch"
        );
        assert_eq!(
            create_matches.get_one::<String>("note").unwrap(),
            "This is a test note"
        );
    }

    #[test]
    fn test_cli_create_with_note_short_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "shards",
            "create",
            "feature-branch",
            "-n",
            "Short note",
        ]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("note").unwrap(),
            "Short note"
        );
    }

    #[test]
    fn test_cli_create_without_note() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "create", "feature-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        // Note should be None when not specified
        assert!(create_matches.get_one::<String>("note").is_none());
    }

    #[test]
    fn test_cli_quiet_flag_short() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "-q", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));
    }

    #[test]
    fn test_cli_quiet_flag_long() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "--quiet", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));
    }

    #[test]
    fn test_cli_quiet_flag_with_subcommand_args() {
        let app = build_cli();
        // Quiet flag should work regardless of position (before subcommand)
        let matches = app.try_get_matches_from(vec!["shards", "-q", "create", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));

        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_quiet_flag_default_false() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(!matches.get_flag("quiet"));
    }

    #[test]
    fn test_cli_quiet_flag_after_subcommand() {
        let app = build_cli();
        // Global flag should work after subcommand too
        let matches = app.try_get_matches_from(vec!["shards", "list", "-q"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));
    }

    #[test]
    fn test_cli_quiet_flag_after_subcommand_long() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "list", "--quiet"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));
    }

    #[test]
    fn test_cli_quiet_flag_after_subcommand_args() {
        let app = build_cli();
        // Test: shards create test-branch --quiet
        let matches = app.try_get_matches_from(vec!["shards", "create", "test-branch", "--quiet"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));

        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_quiet_flag_with_destroy_force() {
        let app = build_cli();
        // Test quiet flag combined with other flags
        let matches =
            app.try_get_matches_from(vec!["shards", "-q", "destroy", "test-branch", "--force"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("quiet"));

        let destroy_matches = matches.subcommand_matches("destroy").unwrap();
        assert!(destroy_matches.get_flag("force"));
        assert_eq!(
            destroy_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_cd_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "cd", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let cd_matches = matches.subcommand_matches("cd").unwrap();
        assert_eq!(
            cd_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_cd_requires_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "cd"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_code_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "code", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let code_matches = matches.subcommand_matches("code").unwrap();
        assert_eq!(
            code_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_code_command_with_editor() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["shards", "code", "test-branch", "--editor", "vim"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let code_matches = matches.subcommand_matches("code").unwrap();
        assert_eq!(
            code_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert_eq!(code_matches.get_one::<String>("editor").unwrap(), "vim");
    }

    #[test]
    fn test_cli_focus_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "focus", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let focus_matches = matches.subcommand_matches("focus").unwrap();
        assert_eq!(
            focus_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_focus_requires_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "focus"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_diff_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "diff", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let diff_matches = matches.subcommand_matches("diff").unwrap();
        assert_eq!(
            diff_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert!(!diff_matches.get_flag("staged"));
    }

    #[test]
    fn test_cli_diff_requires_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "diff"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_diff_with_staged_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "diff", "test-branch", "--staged"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let diff_matches = matches.subcommand_matches("diff").unwrap();
        assert_eq!(
            diff_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert!(diff_matches.get_flag("staged"));
    }

    #[test]
    fn test_cli_commits_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "commits", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let commits_matches = matches.subcommand_matches("commits").unwrap();
        assert_eq!(
            commits_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        // Default count is 10
        assert_eq!(*commits_matches.get_one::<usize>("count").unwrap(), 10);
    }

    #[test]
    fn test_cli_commits_with_count_long() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["shards", "commits", "test-branch", "--count", "5"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let commits_matches = matches.subcommand_matches("commits").unwrap();
        assert_eq!(
            commits_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert_eq!(*commits_matches.get_one::<usize>("count").unwrap(), 5);
    }

    #[test]
    fn test_cli_commits_with_count_short() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "commits", "test-branch", "-n", "3"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let commits_matches = matches.subcommand_matches("commits").unwrap();
        assert_eq!(
            commits_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert_eq!(*commits_matches.get_one::<usize>("count").unwrap(), 3);
    }

    #[test]
    fn test_cli_commits_requires_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "commits"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "open", "--all"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(open_matches.get_flag("all"));
        assert!(open_matches.get_one::<String>("branch").is_none());
    }

    #[test]
    fn test_cli_open_all_conflicts_with_branch() {
        let app = build_cli();
        // --all and branch should conflict
        let matches = app.try_get_matches_from(vec!["shards", "open", "--all", "some-branch"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_all_with_agent() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["shards", "open", "--all", "--agent", "claude"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(open_matches.get_flag("all"));
        assert_eq!(open_matches.get_one::<String>("agent").unwrap(), "claude");
    }

    #[test]
    fn test_cli_stop_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "stop", "--all"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let stop_matches = matches.subcommand_matches("stop").unwrap();
        assert!(stop_matches.get_flag("all"));
    }

    #[test]
    fn test_cli_stop_all_conflicts_with_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "stop", "--all", "some-branch"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_requires_branch_or_all() {
        let app = build_cli();
        // `shards open` with no args should fail at CLI level
        let matches = app.try_get_matches_from(vec!["shards", "open"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_stop_requires_branch_or_all() {
        let app = build_cli();
        // `shards stop` with no args should fail at CLI level
        let matches = app.try_get_matches_from(vec!["shards", "stop"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_with_branch_no_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "open", "my-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(!open_matches.get_flag("all"));
        assert_eq!(
            open_matches.get_one::<String>("branch").unwrap(),
            "my-branch"
        );
    }

    #[test]
    fn test_cli_stop_with_branch_no_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["shards", "stop", "my-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let stop_matches = matches.subcommand_matches("stop").unwrap();
        assert!(!stop_matches.get_flag("all"));
        assert_eq!(
            stop_matches.get_one::<String>("branch").unwrap(),
            "my-branch"
        );
    }
}
