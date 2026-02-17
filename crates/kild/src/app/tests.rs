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
    let matches = app.try_get_matches_from(vec!["kild", "health", "--watch", "--interval", "10"]);
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
    let matches = app.try_get_matches_from(vec!["kild", "-v", "destroy", "test-branch", "--force"]);
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
    let matches = app.try_get_matches_from(vec!["kild", "code", "test-branch", "--editor", "vim"]);
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
fn test_cli_hide_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "hide", "test-branch"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let hide_matches = matches.subcommand_matches("hide").unwrap();
    assert_eq!(
        hide_matches.get_one::<String>("branch").unwrap(),
        "test-branch"
    );
    assert!(!hide_matches.get_flag("all"));
}

#[test]
fn test_cli_hide_all_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "hide", "--all"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let hide_matches = matches.subcommand_matches("hide").unwrap();
    assert!(hide_matches.get_flag("all"));
    assert!(hide_matches.get_one::<String>("branch").is_none());
}

#[test]
fn test_cli_hide_all_conflicts_with_branch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "hide", "--all", "some-branch"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_hide_requires_branch_or_all() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "hide"]);
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
    let matches = app.try_get_matches_from(vec!["kild", "commits", "test-branch", "--count", "5"]);
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
    let matches = app.try_get_matches_from(vec!["kild", "create", "feature-auth", "-b", "develop"]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert_eq!(create_matches.get_one::<String>("base").unwrap(), "develop");
}

#[test]
fn test_cli_create_with_no_fetch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "create", "feature-auth", "--no-fetch"]);
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

// --- pr command tests ---

#[test]
fn test_cli_pr_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "pr", "test-branch"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let pr_matches = matches.subcommand_matches("pr").unwrap();
    assert_eq!(
        pr_matches.get_one::<String>("branch").unwrap(),
        "test-branch"
    );
    assert!(!pr_matches.get_flag("json"));
    assert!(!pr_matches.get_flag("refresh"));
}

#[test]
fn test_cli_pr_with_json_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "pr", "test-branch", "--json"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let pr_matches = matches.subcommand_matches("pr").unwrap();
    assert!(pr_matches.get_flag("json"));
}

#[test]
fn test_cli_pr_with_refresh_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "pr", "test-branch", "--refresh"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let pr_matches = matches.subcommand_matches("pr").unwrap();
    assert!(pr_matches.get_flag("refresh"));
}

#[test]
fn test_cli_pr_requires_branch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "pr"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_pr_with_json_and_refresh() {
    let app = build_cli();
    let matches =
        app.try_get_matches_from(vec!["kild", "pr", "test-branch", "--json", "--refresh"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let pr_matches = matches.subcommand_matches("pr").unwrap();
    assert!(pr_matches.get_flag("json"));
    assert!(pr_matches.get_flag("refresh"));
}

#[test]
fn test_cli_agent_status_with_branch_and_status() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "agent-status", "my-branch", "working"]);
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
fn test_cli_agent_status_with_notify_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "agent-status",
        "my-branch",
        "waiting",
        "--notify",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("agent-status").unwrap();
    assert!(sub.get_flag("notify"));
    assert!(!sub.get_flag("self"));
}

#[test]
fn test_cli_agent_status_with_self_and_notify() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "agent-status",
        "--self",
        "--notify",
        "waiting",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("agent-status").unwrap();
    assert!(sub.get_flag("self"));
    assert!(sub.get_flag("notify"));
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
    let matches = app.try_get_matches_from(vec!["kild", "rebase", "test-branch", "--base", "dev"]);
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
    let matches = app.try_get_matches_from(vec!["kild", "sync", "test-branch", "--base", "dev"]);
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

#[test]
fn test_cli_create_no_agent_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "create", "my-branch", "--no-agent"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("no-agent"));
    assert_eq!(
        create_matches.get_one::<String>("branch").unwrap(),
        "my-branch"
    );
}

#[test]
fn test_cli_create_no_agent_conflicts_with_agent() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "my-branch",
        "--no-agent",
        "--agent",
        "claude",
    ]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_create_no_agent_conflicts_with_startup_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "my-branch",
        "--no-agent",
        "--startup-command",
        "some-cmd",
    ]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_create_no_agent_conflicts_with_flags() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "my-branch",
        "--no-agent",
        "--flags",
        "--trust-all-tools",
    ]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_create_no_agent_with_note() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "my-branch",
        "--no-agent",
        "--note",
        "manual work",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("no-agent"));
    assert_eq!(
        create_matches.get_one::<String>("note").unwrap(),
        "manual work"
    );
}

#[test]
fn test_cli_create_no_agent_with_base() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "my-branch",
        "--no-agent",
        "--base",
        "develop",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("no-agent"));
    assert_eq!(create_matches.get_one::<String>("base").unwrap(), "develop");
}

#[test]
fn test_cli_create_no_agent_with_no_fetch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "my-branch",
        "--no-agent",
        "--no-fetch",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("no-agent"));
    assert!(create_matches.get_flag("no-fetch"));
}

// --- stats command tests ---

#[test]
fn test_cli_stats_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "test-branch"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("stats").unwrap();
    assert_eq!(sub.get_one::<String>("branch").unwrap(), "test-branch");
    assert!(!sub.get_flag("json"));
    assert!(!sub.get_flag("all"));
    assert!(sub.get_one::<String>("base").is_none());
}

#[test]
fn test_cli_stats_with_json() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "test-branch", "--json"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("stats").unwrap();
    assert!(sub.get_flag("json"));
}

#[test]
fn test_cli_stats_all_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "--all"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("stats").unwrap();
    assert!(sub.get_flag("all"));
    assert!(sub.get_one::<String>("branch").is_none());
}

#[test]
fn test_cli_stats_all_with_json() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "--all", "--json"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("stats").unwrap();
    assert!(sub.get_flag("all"));
    assert!(sub.get_flag("json"));
}

#[test]
fn test_cli_stats_all_conflicts_with_branch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "--all", "some-branch"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_stats_with_base() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "test-branch", "--base", "dev"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("stats").unwrap();
    assert_eq!(sub.get_one::<String>("base").unwrap(), "dev");
}

#[test]
fn test_cli_stats_with_base_short() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats", "test-branch", "-b", "dev"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("stats").unwrap();
    assert_eq!(sub.get_one::<String>("base").unwrap(), "dev");
}

#[test]
fn test_cli_stats_requires_branch_or_all() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "stats"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_overlaps_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "overlaps"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("overlaps").unwrap();
    assert!(!sub.get_flag("json"));
    assert!(sub.get_one::<String>("base").is_none());
}

#[test]
fn test_cli_overlaps_json_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "overlaps", "--json"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("overlaps").unwrap();
    assert!(sub.get_flag("json"));
}

#[test]
fn test_cli_overlaps_base_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "overlaps", "--base", "dev"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("overlaps").unwrap();
    assert_eq!(sub.get_one::<String>("base").unwrap(), "dev");
}

// --- completions command tests ---

#[test]
fn test_cli_completions_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "completions", "bash"]);
    assert!(matches.is_ok());
}

#[test]
fn test_cli_completions_requires_shell() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "completions"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_completions_rejects_invalid_shell() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "completions", "invalid"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_overlaps_base_short_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "overlaps", "-b", "develop"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("overlaps").unwrap();
    assert_eq!(sub.get_one::<String>("base").unwrap(), "develop");
}

// --- yolo flag tests ---

#[test]
fn test_cli_create_yolo_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "create", "test-branch", "--yolo"]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("yolo"));
}

#[test]
fn test_cli_create_yolo_conflicts_with_no_agent() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "test-branch",
        "--yolo",
        "--no-agent",
    ]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_create_yolo_with_agent() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "test-branch",
        "--yolo",
        "--agent",
        "kiro",
    ]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("yolo"));
    assert_eq!(create_matches.get_one::<String>("agent").unwrap(), "kiro");
}

#[test]
fn test_cli_create_yolo_with_flags() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild",
        "create",
        "test-branch",
        "--yolo",
        "--flags",
        "--verbose",
    ]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("yolo"));
    assert_eq!(
        create_matches.get_one::<String>("flags").unwrap(),
        "--verbose"
    );
}

#[test]
fn test_cli_open_yolo_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "open", "test-branch", "--yolo"]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let open_matches = matches.subcommand_matches("open").unwrap();
    assert!(open_matches.get_flag("yolo"));
}

#[test]
fn test_cli_open_yolo_conflicts_with_no_agent() {
    let app = build_cli();
    let matches =
        app.try_get_matches_from(vec!["kild", "open", "test-branch", "--yolo", "--no-agent"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_open_yolo_with_all() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "open", "--all", "--yolo"]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let open_matches = matches.subcommand_matches("open").unwrap();
    assert!(open_matches.get_flag("yolo"));
    assert!(open_matches.get_flag("all"));
}

#[test]
fn test_cli_no_color_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "--no-color", "list"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    assert!(matches.get_flag("no-color"));
}

#[test]
fn test_cli_no_color_flag_after_subcommand() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "list", "--no-color"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    assert!(matches.get_flag("no-color"));
}

#[test]
fn test_cli_no_color_default_false() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "list"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    assert!(!matches.get_flag("no-color"));
}

// --- init-hooks command tests ---

#[test]
fn test_cli_init_hooks_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "init-hooks", "opencode"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("init-hooks").unwrap();
    assert_eq!(sub.get_one::<String>("agent").unwrap(), "opencode");
    assert!(!sub.get_flag("no-install"));
}

#[test]
fn test_cli_init_hooks_requires_agent() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "init-hooks"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_init_hooks_rejects_invalid_agent() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "init-hooks", "codex"]);
    assert!(matches.is_err());
}

#[test]
fn test_cli_init_hooks_no_install_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "init-hooks", "opencode", "--no-install"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let sub = matches.subcommand_matches("init-hooks").unwrap();
    assert_eq!(sub.get_one::<String>("agent").unwrap(), "opencode");
    assert!(sub.get_flag("no-install"));
}
