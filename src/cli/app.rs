use clap::{Arg, ArgMatches, Command};

pub fn build_cli() -> Command {
    Command::new("shards")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage parallel AI development agents in isolated Git worktrees")
        .long_about("Shards creates isolated git worktrees and launches AI coding agents in dedicated terminal windows. Each 'shard' is a disposable work context where an AI agent can operate autonomously without disrupting your main working directory.")
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
        )
        .subcommand(
            Command::new("list")
                .about("List all shards for current project")
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
        )
        .subcommand(
            Command::new("cleanup")
                .about("Clean up orphaned resources (branches, worktrees, sessions)")
        )
}

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
}
