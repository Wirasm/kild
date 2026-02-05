use clap::{Arg, ArgAction, ArgMatches, Command};

pub fn build_cli() -> Command {
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
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("create")
                .about("Create a new kild with git worktree and launch agent")
                .arg(
                    Arg::new("branch")
                        .help("Branch name for the kild")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("agent")
                        .long("agent")
                        .short('a')
                        .help("AI agent to launch (overrides config)")
                        .value_parser(["amp", "claude", "kiro", "gemini", "codex", "opencode"])
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
                        .help("Description of what this kild is for (shown in list/status output)")
                )
                .arg(
                    Arg::new("base")
                        .long("base")
                        .short('b')
                        .help("Base branch to create worktree from (overrides config, default: main)")
                )
                .arg(
                    Arg::new("no-fetch")
                        .long("no-fetch")
                        .help("Skip fetching from remote before creating worktree")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("list")
                .about("List all kild for current project")
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
                        .help("Branch name of the kild")
                        .required(true)
                        .index(1)
                )
        )
        .subcommand(
            Command::new("destroy")
                .about("Remove kild completely")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to destroy")
                        .required_unless_present("all")
                        .index(1)
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .short('f')
                        .help("Force destroy, bypassing git uncommitted changes check and confirmation prompt")
                        .action(ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Destroy all kild for current project")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
                )
        )
        .subcommand(
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
                        .index(1)
                )
        )
        .subcommand(
            Command::new("open")
                .about("Open a new agent terminal in an existing kild (additive)")
                .arg(
                    Arg::new("branch")
                        .help("Branch name or kild identifier")
                        .index(1)
                        .required_unless_present("all")
                )
                .arg(
                    Arg::new("agent")
                        .long("agent")
                        .short('a')
                        .help("Agent to launch (default: kild's original agent)")
                        .value_parser(["amp", "claude", "kiro", "gemini", "codex", "opencode"])
                )
                .arg(
                    Arg::new("no-agent")
                        .long("no-agent")
                        .help("Open a bare terminal with default shell instead of an agent")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("agent")
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Open agents in all stopped kild")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
                )
        )
        .subcommand(
            Command::new("stop")
                .about("Stop agent(s) in a kild without destroying the worktree")
                .arg(
                    Arg::new("branch")
                        .help("Branch name or kild identifier")
                        .index(1)
                        .required_unless_present("all")
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Stop all running kild")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
                )
        )
        .subcommand(
            Command::new("code")
                .about("Open kild's worktree in your code editor")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to open")
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
                .about("Bring a kild's terminal window to the foreground")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to focus")
                        .required(true)
                        .index(1)
                )
        )
        .subcommand(
            Command::new("diff")
                .about("Show git diff for a kild's worktree")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("staged")
                        .long("staged")
                        .help("Show only staged changes (git diff --staged)")
                        .action(ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("stat")
                        .long("stat")
                        .help("Show unstaged diffstat summary instead of full diff")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("commits")
                .about("Show recent commits in a kild's branch")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild")
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
                .about("Restart agent in existing kild without destroying worktree")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to restart")
                        .required(true)
                        .index(1)
                )
                .arg(
                    Arg::new("agent")
                        .long("agent")
                        .short('a')
                        .help("AI agent to use (overrides current agent)")
                        .value_parser(["amp", "claude", "kiro", "gemini", "codex", "opencode"])
                )
        )
        .subcommand(
            Command::new("status")
                .about("Show detailed status of a kild")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to check")
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
            Command::new("agent-status")
                .about("Report agent activity status (called by agent hooks)")
                .arg(
                    Arg::new("target")
                        .help("Branch name and status (e.g., 'mybranch working') or just status with --self (e.g., 'working')")
                        .required(true)
                        .num_args(1..=2)
                        .value_parser(clap::value_parser!(String))
                )
                .arg(
                    Arg::new("self")
                        .long("self")
                        .help("Auto-detect session from current working directory")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("rebase")
                .about("Rebase a kild's branch onto the base branch")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to rebase")
                        .index(1)
                        .required_unless_present("all")
                )
                .arg(
                    Arg::new("base")
                        .long("base")
                        .short('b')
                        .help("Base branch to rebase onto (overrides config, default: main)")
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Rebase all active kilds")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
                )
        )
        .subcommand(
            Command::new("sync")
                .about("Fetch from remote and rebase a kild's branch onto the base branch")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of the kild to sync")
                        .index(1)
                        .required_unless_present("all")
                )
                .arg(
                    Arg::new("base")
                        .long("base")
                        .short('b')
                        .help("Base branch to rebase onto (overrides config, default: main)")
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Fetch and rebase all active kilds")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("branch")
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
                        .help("Clean worktrees in kild directory that have no session")
                        .action(ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("health")
                .about("Show health status and metrics for kild")
                .arg(
                    Arg::new("branch")
                        .help("Branch name of specific kild to check (optional)")
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
        assert_eq!(app.get_name(), "kild");
    }

    #[test]
    fn test_cli_create_command() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "create", "test-branch", "--agent", "kiro"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.subcommand_matches("list").is_some());
    }

    #[test]
    fn test_cli_list_json_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "list", "--json"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let list_matches = matches.subcommand_matches("list").unwrap();
        assert!(list_matches.get_flag("json"));
    }

    #[test]
    fn test_cli_status_json_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "status", "test-branch", "--json"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "destroy", "test-branch"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "create", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        // Agent is now optional, should be None when not specified
        assert!(create_matches.get_one::<String>("agent").is_none());
    }

    #[test]
    fn test_cli_invalid_agent() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "create", "test-branch", "--agent", "invalid"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_create_with_complex_flags() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild",
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
            app.try_get_matches_from(vec!["kild", "health", "--watch", "--interval", "10"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let health_matches = matches.subcommand_matches("health").unwrap();
        assert!(health_matches.get_flag("watch"));
        assert_eq!(*health_matches.get_one::<u64>("interval").unwrap(), 10);
    }

    #[test]
    fn test_cli_health_default_interval() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "health", "--watch"]);
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
            "kild",
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
        let matches =
            app.try_get_matches_from(vec!["kild", "create", "feature-branch", "-n", "Short note"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "create", "feature-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        // Note should be None when not specified
        assert!(create_matches.get_one::<String>("note").is_none());
    }

    #[test]
    fn test_cli_verbose_flag_short() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "-v", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));
    }

    #[test]
    fn test_cli_verbose_flag_long() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "--verbose", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));
    }

    #[test]
    fn test_cli_verbose_flag_with_subcommand_args() {
        let app = build_cli();
        // Verbose flag should work regardless of position (before subcommand)
        let matches = app.try_get_matches_from(vec!["kild", "-v", "create", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));

        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_verbose_flag_default_false() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "list"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(!matches.get_flag("verbose"));
    }

    #[test]
    fn test_cli_verbose_flag_after_subcommand() {
        let app = build_cli();
        // Global flag should work after subcommand too
        let matches = app.try_get_matches_from(vec!["kild", "list", "-v"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));
    }

    #[test]
    fn test_cli_verbose_flag_after_subcommand_long() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "list", "--verbose"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));
    }

    #[test]
    fn test_cli_verbose_flag_after_subcommand_args() {
        let app = build_cli();
        // Test: kild create test-branch --verbose
        let matches = app.try_get_matches_from(vec!["kild", "create", "test-branch", "--verbose"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));

        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_verbose_flag_with_destroy_force() {
        let app = build_cli();
        // Test verbose flag combined with other flags
        let matches =
            app.try_get_matches_from(vec!["kild", "-v", "destroy", "test-branch", "--force"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.get_flag("verbose"));

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
        let matches = app.try_get_matches_from(vec!["kild", "cd", "test-branch"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "cd"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_code_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "code", "test-branch"]);
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
            app.try_get_matches_from(vec!["kild", "code", "test-branch", "--editor", "vim"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "focus", "test-branch"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "focus"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_diff_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "diff", "test-branch"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "diff"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_diff_with_staged_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "diff", "test-branch", "--staged"]);
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
    fn test_cli_diff_with_stat_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "diff", "test-branch", "--stat"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let diff_matches = matches.subcommand_matches("diff").unwrap();
        assert_eq!(
            diff_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
        assert!(diff_matches.get_flag("stat"));
        assert!(!diff_matches.get_flag("staged"));
    }

    #[test]
    fn test_cli_commits_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "commits", "test-branch"]);
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
            app.try_get_matches_from(vec!["kild", "commits", "test-branch", "--count", "5"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "commits", "test-branch", "-n", "3"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "commits"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "open", "--all"]);
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
        let matches = app.try_get_matches_from(vec!["kild", "open", "--all", "some-branch"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_all_with_agent() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "open", "--all", "--agent", "claude"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(open_matches.get_flag("all"));
        assert_eq!(open_matches.get_one::<String>("agent").unwrap(), "claude");
    }

    #[test]
    fn test_cli_stop_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "stop", "--all"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let stop_matches = matches.subcommand_matches("stop").unwrap();
        assert!(stop_matches.get_flag("all"));
    }

    #[test]
    fn test_cli_stop_all_conflicts_with_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "stop", "--all", "some-branch"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_requires_branch_or_all() {
        let app = build_cli();
        // `kild open` with no args should fail at CLI level
        let matches = app.try_get_matches_from(vec!["kild", "open"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_stop_requires_branch_or_all() {
        let app = build_cli();
        // `kild stop` with no args should fail at CLI level
        let matches = app.try_get_matches_from(vec!["kild", "stop"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_with_branch_no_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "open", "my-branch"]);
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
    fn test_cli_open_no_agent_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "open", "my-branch", "--no-agent"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(open_matches.get_flag("no-agent"));
        assert!(!open_matches.get_flag("all"));
    }

    #[test]
    fn test_cli_open_no_agent_conflicts_with_agent() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild",
            "open",
            "my-branch",
            "--no-agent",
            "--agent",
            "claude",
        ]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_open_no_agent_with_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "open", "my-branch", "--no-agent"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(open_matches.get_flag("no-agent"));
        assert_eq!(
            open_matches.get_one::<String>("branch").unwrap(),
            "my-branch"
        );
    }

    #[test]
    fn test_cli_open_no_agent_with_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "open", "--all", "--no-agent"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let open_matches = matches.subcommand_matches("open").unwrap();
        assert!(open_matches.get_flag("no-agent"));
        assert!(open_matches.get_flag("all"));
    }

    #[test]
    fn test_cli_stop_with_branch_no_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "stop", "my-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let stop_matches = matches.subcommand_matches("stop").unwrap();
        assert!(!stop_matches.get_flag("all"));
        assert_eq!(
            stop_matches.get_one::<String>("branch").unwrap(),
            "my-branch"
        );
    }

    #[test]
    fn test_cli_destroy_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "destroy", "--all"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let destroy_matches = matches.subcommand_matches("destroy").unwrap();
        assert!(destroy_matches.get_flag("all"));
        assert!(destroy_matches.get_one::<String>("branch").is_none());
    }

    #[test]
    fn test_cli_destroy_all_conflicts_with_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "destroy", "--all", "some-branch"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_destroy_all_with_force() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "destroy", "--all", "--force"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let destroy_matches = matches.subcommand_matches("destroy").unwrap();
        assert!(destroy_matches.get_flag("all"));
        assert!(destroy_matches.get_flag("force"));
    }

    #[test]
    fn test_cli_destroy_requires_branch_or_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "destroy"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_complete_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "complete", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let complete_matches = matches.subcommand_matches("complete").unwrap();
        assert_eq!(
            complete_matches.get_one::<String>("branch").unwrap(),
            "test-branch"
        );
    }

    #[test]
    fn test_cli_complete_rejects_force_flag() {
        let app = build_cli();
        // --force should not be accepted on complete (removed in #188)
        let matches = app.try_get_matches_from(vec!["kild", "complete", "test-branch", "--force"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_complete_requires_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "complete"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_create_with_base_branch() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "create", "feature-auth", "--base", "develop"]);
        assert!(matches.is_ok());
        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(create_matches.get_one::<String>("base").unwrap(), "develop");
    }

    #[test]
    fn test_cli_create_with_base_short_flag() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "create", "feature-auth", "-b", "develop"]);
        assert!(matches.is_ok());
        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert_eq!(create_matches.get_one::<String>("base").unwrap(), "develop");
    }

    #[test]
    fn test_cli_create_with_no_fetch() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "create", "feature-auth", "--no-fetch"]);
        assert!(matches.is_ok());
        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert!(create_matches.get_flag("no-fetch"));
    }

    #[test]
    fn test_cli_create_with_base_and_no_fetch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec![
            "kild",
            "create",
            "feature-auth",
            "--base",
            "develop",
            "--no-fetch",
        ]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_cli_create_no_fetch_default_false() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "create", "feature-auth"]);
        assert!(matches.is_ok());
        let matches = matches.unwrap();
        let create_matches = matches.subcommand_matches("create").unwrap();
        assert!(!create_matches.get_flag("no-fetch"));
        assert!(create_matches.get_one::<String>("base").is_none());
    }

    #[test]
    fn test_cli_agent_status_with_branch_and_status() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "agent-status", "my-branch", "working"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("agent-status").unwrap();
        let targets: Vec<&String> = sub.get_many::<String>("target").unwrap().collect();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], "my-branch");
        assert_eq!(targets[1], "working");
        assert!(!sub.get_flag("self"));
    }

    #[test]
    fn test_cli_agent_status_with_self_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "agent-status", "--self", "idle"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("agent-status").unwrap();
        let targets: Vec<&String> = sub.get_many::<String>("target").unwrap().collect();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "idle");
        assert!(sub.get_flag("self"));
    }

    #[test]
    fn test_cli_agent_status_requires_at_least_one_target() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "agent-status"]);
        assert!(matches.is_err());
    }

    // --- rebase command tests ---

    #[test]
    fn test_cli_rebase_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "rebase", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("rebase").unwrap();
        assert_eq!(sub.get_one::<String>("branch").unwrap(), "test-branch");
        assert!(!sub.get_flag("all"));
        assert!(sub.get_one::<String>("base").is_none());
    }

    #[test]
    fn test_cli_rebase_requires_branch_or_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "rebase"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_rebase_with_base() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "rebase", "test-branch", "--base", "dev"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("rebase").unwrap();
        assert_eq!(sub.get_one::<String>("branch").unwrap(), "test-branch");
        assert_eq!(sub.get_one::<String>("base").unwrap(), "dev");
    }

    #[test]
    fn test_cli_rebase_with_base_short() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "rebase", "test-branch", "-b", "dev"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("rebase").unwrap();
        assert_eq!(sub.get_one::<String>("base").unwrap(), "dev");
    }

    #[test]
    fn test_cli_rebase_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "rebase", "--all"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("rebase").unwrap();
        assert!(sub.get_flag("all"));
        assert!(sub.get_one::<String>("branch").is_none());
    }

    #[test]
    fn test_cli_rebase_all_conflicts_with_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "rebase", "--all", "some-branch"]);
        assert!(matches.is_err());
    }

    // --- sync command tests ---

    #[test]
    fn test_cli_sync_command() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "sync", "test-branch"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("sync").unwrap();
        assert_eq!(sub.get_one::<String>("branch").unwrap(), "test-branch");
        assert!(!sub.get_flag("all"));
        assert!(sub.get_one::<String>("base").is_none());
    }

    #[test]
    fn test_cli_sync_requires_branch_or_all() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "sync"]);
        assert!(matches.is_err());
    }

    #[test]
    fn test_cli_sync_with_base() {
        let app = build_cli();
        let matches =
            app.try_get_matches_from(vec!["kild", "sync", "test-branch", "--base", "dev"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("sync").unwrap();
        assert_eq!(sub.get_one::<String>("branch").unwrap(), "test-branch");
        assert_eq!(sub.get_one::<String>("base").unwrap(), "dev");
    }

    #[test]
    fn test_cli_sync_all_flag() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "sync", "--all"]);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        let sub = matches.subcommand_matches("sync").unwrap();
        assert!(sub.get_flag("all"));
        assert!(sub.get_one::<String>("branch").is_none());
    }

    #[test]
    fn test_cli_sync_all_conflicts_with_branch() {
        let app = build_cli();
        let matches = app.try_get_matches_from(vec!["kild", "sync", "--all", "some-branch"]);
        assert!(matches.is_err());
    }
}
