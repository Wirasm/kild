use super::*;

fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

#[test]
fn test_version() {
    let a = args("-V");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::Version));
}

#[test]
fn test_strip_socket_flag() {
    let a = args("-L kild split-window -h -t %0");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::SplitWindow(_)));
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.horizontal);
        assert_eq!(sw.target, Some("%0"));
    }
}

#[test]
fn test_split_window_defaults() {
    let a = args("split-window");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(!sw.horizontal);
        assert!(sw.target.is_none());
        assert!(sw.size.is_none());
        assert!(!sw.print_info);
        assert!(sw.format.is_none());
        assert!(sw.command.is_empty());
    } else {
        panic!("expected SplitWindow");
    }
}

#[test]
fn test_send_keys_with_target() {
    let a = args("send-keys -t %1 echo hello Enter");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SendKeys(sk) = cmd {
        assert_eq!(sk.target, Some("%1"));
        assert_eq!(sk.keys, vec!["echo", "hello", "Enter"]);
    } else {
        panic!("expected SendKeys");
    }
}

#[test]
fn test_send_keys_no_target() {
    let a = args("send-keys ls Enter");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SendKeys(sk) = cmd {
        assert!(sk.target.is_none());
        assert_eq!(sk.keys, vec!["ls", "Enter"]);
    } else {
        panic!("expected SendKeys");
    }
}

#[test]
fn test_set_option_pane_scope() {
    let a = args("set-option -p -t %0 pane-border-style fg=blue");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SetOption(so) = cmd {
        assert!(matches!(so.scope, OptionScope::Pane));
        assert_eq!(so.target, Some("%0"));
        assert_eq!(so.key, "pane-border-style");
        assert_eq!(so.value, "fg=blue");
    } else {
        panic!("expected SetOption");
    }
}

#[test]
fn test_has_session() {
    let a = args("has-session -t claude-swarm");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::HasSession(hs) = cmd {
        assert_eq!(hs.target, "claude-swarm");
    } else {
        panic!("expected HasSession");
    }
}

#[test]
fn test_display_message_print() {
    let a: Vec<String> = vec!["display-message", "-t", "%0", "-p", "#{pane_id}"]
        .into_iter()
        .map(String::from)
        .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::DisplayMessage(dm) = cmd {
        assert!(dm.print);
        assert_eq!(dm.target, Some("%0"));
        assert_eq!(dm.format, Some("#{pane_id}"));
    } else {
        panic!("expected DisplayMessage");
    }
}

#[test]
fn test_select_pane_with_style_and_title() {
    let a: Vec<String> = vec![
        "select-pane",
        "-t",
        "%1",
        "-P",
        "bg=default,fg=blue",
        "-T",
        "researcher",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SelectPane(sp) = cmd {
        assert_eq!(sp.target, Some("%1"));
        assert_eq!(sp.style, Some("bg=default,fg=blue"));
        assert_eq!(sp.title, Some("researcher"));
    } else {
        panic!("expected SelectPane");
    }
}

#[test]
fn test_new_session_full() {
    let a: Vec<String> = vec![
        "new-session",
        "-d",
        "-s",
        "mysess",
        "-n",
        "main",
        "-P",
        "-F",
        "#{pane_id}",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::NewSession(ns) = cmd {
        assert!(ns.detached);
        assert_eq!(ns.session_name, Some("mysess"));
        assert_eq!(ns.window_name, Some("main"));
        assert!(ns.print_info);
        assert_eq!(ns.format, Some("#{pane_id}"));
    } else {
        panic!("expected NewSession");
    }
}

#[test]
fn test_resize_pane() {
    let a = args("resize-pane -t %0 -x 30% -y 50%");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::ResizePane(rp) = cmd {
        assert_eq!(rp.target, Some("%0"));
        assert_eq!(rp.width, Some("30%"));
        assert_eq!(rp.height, Some("50%"));
    } else {
        panic!("expected ResizePane");
    }
}

#[test]
fn test_join_pane() {
    let a = args("join-pane -h -s %1 -t kild:0");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::JoinPane(jp) = cmd {
        assert!(jp.horizontal);
        assert_eq!(jp.source, Some("%1"));
        assert_eq!(jp.target, Some("kild:0"));
    } else {
        panic!("expected JoinPane");
    }
}

#[test]
fn test_break_pane() {
    let a: Vec<String> = vec!["break-pane", "-d", "-s", "%1", "-t", "claude-hidden:"]
        .into_iter()
        .map(String::from)
        .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::BreakPane(bp) = cmd {
        assert!(bp.detached);
        assert_eq!(bp.source, Some("%1"));
        assert_eq!(bp.target, Some("claude-hidden:"));
    } else {
        panic!("expected BreakPane");
    }
}

#[test]
fn test_unknown_command() {
    let a = args("foobar");
    let result = parse(&a);
    assert!(result.is_err());
}

#[test]
fn test_no_subcommand() {
    let result = parse(&[]);
    assert!(result.is_err());
}

#[test]
fn test_alias_splitw() {
    let a = args("splitw -h");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::SplitWindow(_)));
}

#[test]
fn test_alias_send() {
    let a = args("send -t %0 hello");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::SendKeys(_)));
}

#[test]
fn test_select_layout() {
    let a = args("select-layout -t kild:0 main-vertical");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SelectLayout(sl) = cmd {
        assert_eq!(sl.target, Some("kild:0"));
        assert_eq!(sl.layout, "main-vertical");
    } else {
        panic!("expected SelectLayout");
    }
}

// --- split-window full flags ---

#[test]
fn test_split_window_all_flags() {
    let a = args("split-window -h -t %2 -l 30% -P -F #{pane_id}");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.horizontal);
        assert_eq!(sw.target, Some("%2"));
        assert_eq!(sw.size, Some("30%"));
        assert!(sw.print_info);
        assert_eq!(sw.format, Some("#{pane_id}"));
    } else {
        panic!("expected SplitWindow");
    }
}

#[test]
fn test_split_window_vertical_explicit() {
    let a = args("split-window -v");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(!sw.horizontal);
    } else {
        panic!("expected SplitWindow");
    }
}

// --- split-window with shell command ---

#[test]
fn test_split_window_command_after_double_dash() {
    let a: Vec<String> = vec![
        "split-window",
        "-P",
        "-F",
        "#{pane_id}",
        "--",
        "/usr/local/bin/claude",
        "--agent-type",
        "researcher",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.print_info);
        assert_eq!(sw.format, Some("#{pane_id}"));
        assert_eq!(
            sw.command,
            vec!["/usr/local/bin/claude", "--agent-type", "researcher"]
        );
    } else {
        panic!("expected SplitWindow");
    }
}

#[test]
fn test_split_window_command_as_positional() {
    let a: Vec<String> = vec!["split-window", "-d", "-P", "/bin/sh", "-c", "echo hello"]
        .into_iter()
        .map(String::from)
        .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.print_info);
        assert_eq!(sw.command, vec!["/bin/sh", "-c", "echo hello"]);
    } else {
        panic!("expected SplitWindow");
    }
}

#[test]
fn test_split_window_detached_flag() {
    let a = args("split-window -d -P");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.print_info);
        assert!(sw.command.is_empty());
    } else {
        panic!("expected SplitWindow");
    }
}

#[test]
fn test_split_window_command_with_all_flags() {
    let a: Vec<String> = vec![
        "split-window",
        "-d",
        "-h",
        "-t",
        "%0",
        "-l",
        "50%",
        "-P",
        "-F",
        "#{pane_id}",
        "--",
        "node",
        "script.js",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.horizontal);
        assert_eq!(sw.target, Some("%0"));
        assert_eq!(sw.size, Some("50%"));
        assert!(sw.print_info);
        assert_eq!(sw.format, Some("#{pane_id}"));
        assert_eq!(sw.command, vec!["node", "script.js"]);
    } else {
        panic!("expected SplitWindow");
    }
}

#[test]
fn test_split_window_empty_command_after_double_dash() {
    let a: Vec<String> = vec!["split-window", "--"]
        .into_iter()
        .map(String::from)
        .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SplitWindow(sw) = cmd {
        assert!(sw.command.is_empty());
    } else {
        panic!("expected SplitWindow");
    }
}

// --- list-panes ---

#[test]
fn test_list_panes_defaults() {
    let a = args("list-panes");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::ListPanes(lp) = cmd {
        assert!(lp.target.is_none());
        assert!(lp.format.is_none());
    } else {
        panic!("expected ListPanes");
    }
}

#[test]
fn test_list_panes_with_target_and_format() {
    let a = args("list-panes -t kild:0 -F #{pane_id}");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::ListPanes(lp) = cmd {
        assert_eq!(lp.target, Some("kild:0"));
        assert_eq!(lp.format, Some("#{pane_id}"));
    } else {
        panic!("expected ListPanes");
    }
}

#[test]
fn test_alias_lsp() {
    let a = args("lsp -F #{pane_id}");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::ListPanes(_)));
}

// --- kill-pane ---

#[test]
fn test_kill_pane_with_target() {
    let a = args("kill-pane -t %3");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::KillPane(kp) = cmd {
        assert_eq!(kp.target, Some("%3"));
    } else {
        panic!("expected KillPane");
    }
}

#[test]
fn test_kill_pane_no_target() {
    let a = args("kill-pane");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::KillPane(kp) = cmd {
        assert!(kp.target.is_none());
    } else {
        panic!("expected KillPane");
    }
}

#[test]
fn test_alias_killp() {
    let a = args("killp -t %1");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::KillPane(_)));
}

// --- display-message alias ---

#[test]
fn test_alias_display() {
    let a = args("display #{pane_id}");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::DisplayMessage(dm) = cmd {
        assert_eq!(dm.format, Some("#{pane_id}"));
    } else {
        panic!("expected DisplayMessage");
    }
}

#[test]
fn test_display_message_no_args() {
    let a = args("display-message");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::DisplayMessage(dm) = cmd {
        assert!(dm.target.is_none());
        assert!(!dm.print);
        assert!(dm.format.is_none());
    } else {
        panic!("expected DisplayMessage");
    }
}

// --- select-pane aliases and defaults ---

#[test]
fn test_select_pane_target_only() {
    let a = args("select-pane -t %0");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SelectPane(sp) = cmd {
        assert_eq!(sp.target, Some("%0"));
        assert!(sp.style.is_none());
        assert!(sp.title.is_none());
    } else {
        panic!("expected SelectPane");
    }
}

#[test]
fn test_alias_selectp() {
    let a = args("selectp -t %0");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::SelectPane(_)));
}

// --- set-option scopes ---

#[test]
fn test_set_option_window_scope() {
    let a = args("set-option -w pane-border-format test-val");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SetOption(so) = cmd {
        assert!(matches!(so.scope, OptionScope::Window));
        assert!(so.target.is_none());
        assert_eq!(so.key, "pane-border-format");
        assert_eq!(so.value, "test-val");
    } else {
        panic!("expected SetOption");
    }
}

#[test]
fn test_set_option_session_scope_default() {
    let a = args("set-option my-option my-value");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SetOption(so) = cmd {
        assert!(matches!(so.scope, OptionScope::Session));
        assert_eq!(so.key, "my-option");
        assert_eq!(so.value, "my-value");
    } else {
        panic!("expected SetOption");
    }
}

#[test]
fn test_set_option_missing_value() {
    let a = args("set-option only-key");
    let result = parse(&a);
    assert!(result.is_err());
}

#[test]
fn test_alias_set() {
    let a = args("set -p my-key my-val");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::SetOption(_)));
}

// --- select-layout alias ---

#[test]
fn test_alias_selectl() {
    let a = args("selectl tiled");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SelectLayout(sl) = cmd {
        assert!(sl.target.is_none());
        assert_eq!(sl.layout, "tiled");
    } else {
        panic!("expected SelectLayout");
    }
}

#[test]
fn test_select_layout_missing_layout() {
    let a = args("select-layout");
    let result = parse(&a);
    assert!(result.is_err());
}

// --- resize-pane alias and defaults ---

#[test]
fn test_resize_pane_defaults() {
    let a = args("resize-pane");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::ResizePane(rp) = cmd {
        assert!(rp.target.is_none());
        assert!(rp.width.is_none());
        assert!(rp.height.is_none());
    } else {
        panic!("expected ResizePane");
    }
}

#[test]
fn test_alias_resizep() {
    let a = args("resizep -x 50%");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::ResizePane(_)));
}

// --- has-session alias and error ---

#[test]
fn test_alias_has() {
    let a = args("has -t mysess");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::HasSession(_)));
}

#[test]
fn test_has_session_missing_target() {
    let a = args("has-session");
    let result = parse(&a);
    assert!(result.is_err());
}

// --- new-session defaults and alias ---

#[test]
fn test_new_session_defaults() {
    let a = args("new-session");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::NewSession(ns) = cmd {
        assert!(!ns.detached);
        assert!(ns.session_name.is_none());
        assert!(ns.window_name.is_none());
        assert!(!ns.print_info);
        assert!(ns.format.is_none());
    } else {
        panic!("expected NewSession");
    }
}

#[test]
fn test_alias_new() {
    let a = args("new -d -s test");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::NewSession(ns) = cmd {
        assert!(ns.detached);
        assert_eq!(ns.session_name, Some("test"));
    } else {
        panic!("expected NewSession");
    }
}

// --- new-window ---

#[test]
fn test_new_window_defaults() {
    let a = args("new-window");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::NewWindow(nw) = cmd {
        assert!(nw.target.is_none());
        assert!(nw.name.is_none());
        assert!(!nw.print_info);
        assert!(nw.format.is_none());
    } else {
        panic!("expected NewWindow");
    }
}

#[test]
fn test_new_window_all_flags() {
    let a: Vec<String> = vec![
        "new-window",
        "-t",
        "kild:0",
        "-n",
        "worker",
        "-P",
        "-F",
        "#{pane_id}",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::NewWindow(nw) = cmd {
        assert_eq!(nw.target, Some("kild:0"));
        assert_eq!(nw.name, Some("worker"));
        assert!(nw.print_info);
        assert_eq!(nw.format, Some("#{pane_id}"));
    } else {
        panic!("expected NewWindow");
    }
}

#[test]
fn test_alias_neww() {
    let a = args("neww -n test");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::NewWindow(_)));
}

// --- list-windows ---

#[test]
fn test_list_windows_defaults() {
    let a = args("list-windows");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::ListWindows(lw) = cmd {
        assert!(lw.target.is_none());
        assert!(lw.format.is_none());
    } else {
        panic!("expected ListWindows");
    }
}

#[test]
fn test_list_windows_with_target_and_format() {
    let a = args("list-windows -t mysess -F #{window_name}");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::ListWindows(lw) = cmd {
        assert_eq!(lw.target, Some("mysess"));
        assert_eq!(lw.format, Some("#{window_name}"));
    } else {
        panic!("expected ListWindows");
    }
}

#[test]
fn test_alias_lsw() {
    let a = args("lsw");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::ListWindows(_)));
}

// --- break-pane alias ---

#[test]
fn test_alias_breakp() {
    let a = args("breakp -d -s %2");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::BreakPane(bp) = cmd {
        assert!(bp.detached);
        assert_eq!(bp.source, Some("%2"));
    } else {
        panic!("expected BreakPane");
    }
}

#[test]
fn test_break_pane_defaults() {
    let a = args("break-pane");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::BreakPane(bp) = cmd {
        assert!(!bp.detached);
        assert!(bp.source.is_none());
        assert!(bp.target.is_none());
    } else {
        panic!("expected BreakPane");
    }
}

// --- join-pane alias and defaults ---

#[test]
fn test_alias_joinp() {
    let a = args("joinp -s %0 -t kild:1");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::JoinPane(_)));
}

#[test]
fn test_join_pane_defaults() {
    let a = args("join-pane");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::JoinPane(jp) = cmd {
        assert!(!jp.horizontal);
        assert!(jp.source.is_none());
        assert!(jp.target.is_none());
    } else {
        panic!("expected JoinPane");
    }
}

// --- capture-pane ---

#[test]
fn test_capture_pane_basic() {
    let a = args("capture-pane -t %1 -p");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::CapturePane(cp) = cmd {
        assert_eq!(cp.target, Some("%1"));
        assert!(cp.print);
        assert_eq!(cp.start_line, None);
    } else {
        panic!("expected CapturePane");
    }
}

#[test]
fn test_capture_pane_with_start_line() {
    let a: Vec<String> = vec!["capture-pane", "-t", "%1", "-p", "-S", "-100"]
        .into_iter()
        .map(String::from)
        .collect();
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::CapturePane(cp) = cmd {
        assert_eq!(cp.target, Some("%1"));
        assert!(cp.print);
        assert_eq!(cp.start_line, Some(-100));
    } else {
        panic!("expected CapturePane");
    }
}

#[test]
fn test_capture_pane_alias() {
    let a = args("capturep -p");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::CapturePane(cp) = cmd {
        assert!(cp.print);
        assert_eq!(cp.target, None);
    } else {
        panic!("expected CapturePane");
    }
}

#[test]
fn test_capture_pane_no_flags() {
    let a = args("capture-pane");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::CapturePane(cp) = cmd {
        assert!(!cp.print);
        assert_eq!(cp.target, None);
        assert_eq!(cp.start_line, None);
    } else {
        panic!("expected CapturePane");
    }
}

// --- Global flag edge cases ---

#[test]
fn test_socket_flag_with_version() {
    let a = args("-L mysock -V");
    let cmd = parse(&a).unwrap();
    assert!(matches!(cmd, TmuxCommand::Version));
}

#[test]
fn test_socket_flag_preserves_remaining_args() {
    let a = args("-L mysock send-keys -t %0 hello Enter");
    let cmd = parse(&a).unwrap();
    if let TmuxCommand::SendKeys(sk) = cmd {
        assert_eq!(sk.target, Some("%0"));
        assert_eq!(sk.keys, vec!["hello", "Enter"]);
    } else {
        panic!("expected SendKeys");
    }
}
