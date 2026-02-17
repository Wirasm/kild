use tracing::debug;

use crate::errors::ShimError;

#[derive(Debug)]
pub enum TmuxCommand<'a> {
    Version,
    SplitWindow(SplitWindowArgs<'a>),
    SendKeys(SendKeysArgs<'a>),
    ListPanes(ListPanesArgs<'a>),
    KillPane(KillPaneArgs<'a>),
    DisplayMessage(DisplayMsgArgs<'a>),
    SelectPane(SelectPaneArgs<'a>),
    SetOption(SetOptionArgs<'a>),
    SelectLayout(SelectLayoutArgs<'a>),
    ResizePane(ResizePaneArgs<'a>),
    HasSession(HasSessionArgs<'a>),
    NewSession(NewSessionArgs<'a>),
    NewWindow(NewWindowArgs<'a>),
    ListWindows(ListWindowsArgs<'a>),
    BreakPane(BreakPaneArgs<'a>),
    JoinPane(JoinPaneArgs<'a>),
    CapturePane(CapturePaneArgs<'a>),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct SplitWindowArgs<'a> {
    pub target: Option<&'a str>,
    pub horizontal: bool,
    pub size: Option<&'a str>,
    pub print_info: bool,
    pub format: Option<&'a str>,
}

#[derive(Debug)]
pub struct SendKeysArgs<'a> {
    pub target: Option<&'a str>,
    pub keys: Vec<&'a str>,
}

#[derive(Debug)]
pub struct ListPanesArgs<'a> {
    pub target: Option<&'a str>,
    pub format: Option<&'a str>,
}

#[derive(Debug)]
pub struct KillPaneArgs<'a> {
    pub target: Option<&'a str>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct DisplayMsgArgs<'a> {
    pub target: Option<&'a str>,
    pub print: bool,
    pub format: Option<&'a str>,
}

#[derive(Debug)]
pub struct SelectPaneArgs<'a> {
    pub target: Option<&'a str>,
    pub style: Option<&'a str>,
    pub title: Option<&'a str>,
}

#[derive(Debug)]
pub struct SetOptionArgs<'a> {
    pub scope: OptionScope,
    pub target: Option<&'a str>,
    pub key: &'a str,
    /// Joined from positional args (allocated). E.g., `["foo", "bar"]` → `"foo bar"`.
    pub value: String,
}

#[derive(Debug)]
pub enum OptionScope {
    Pane,
    Window,
    Session,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct SelectLayoutArgs<'a> {
    pub target: Option<&'a str>,
    pub layout: &'a str,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ResizePaneArgs<'a> {
    pub target: Option<&'a str>,
    pub width: Option<&'a str>,
    pub height: Option<&'a str>,
}

#[derive(Debug)]
pub struct HasSessionArgs<'a> {
    pub target: &'a str,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct NewSessionArgs<'a> {
    pub detached: bool,
    pub session_name: Option<&'a str>,
    pub window_name: Option<&'a str>,
    pub print_info: bool,
    pub format: Option<&'a str>,
}

#[derive(Debug)]
pub struct NewWindowArgs<'a> {
    pub target: Option<&'a str>,
    pub name: Option<&'a str>,
    pub print_info: bool,
    pub format: Option<&'a str>,
}

#[derive(Debug)]
pub struct ListWindowsArgs<'a> {
    pub target: Option<&'a str>,
    pub format: Option<&'a str>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct BreakPaneArgs<'a> {
    pub detached: bool,
    pub source: Option<&'a str>,
    pub target: Option<&'a str>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct JoinPaneArgs<'a> {
    pub horizontal: bool,
    pub source: Option<&'a str>,
    pub target: Option<&'a str>,
}

#[derive(Debug)]
pub struct CapturePaneArgs<'a> {
    pub target: Option<&'a str>,
    pub print: bool,
    /// Start line for capture: `None` = all lines, negative = last N lines
    /// (e.g. `-100` = last 100 lines), non-negative = offset from start.
    pub start_line: Option<i64>,
}

pub fn parse(args: &[String]) -> Result<TmuxCommand<'_>, ShimError> {
    let mut iter = args.iter().peekable();

    // Strip global -L <socket> flag
    let mut filtered: Vec<&str> = Vec::new();
    while let Some(arg) = iter.next() {
        if arg == "-L" {
            // Consume the socket name arg too
            iter.next();
            continue;
        }
        filtered.push(arg);
    }

    let mut iter = filtered.into_iter().peekable();

    // Check for -V before subcommand
    if iter.peek().copied() == Some("-V") {
        return Ok(TmuxCommand::Version);
    }

    let subcmd = iter
        .next()
        .ok_or_else(|| ShimError::parse("no subcommand provided"))?;

    let remaining: Vec<&str> = iter.collect();

    match subcmd {
        "split-window" | "splitw" => parse_split_window(&remaining),
        "send-keys" | "send" => parse_send_keys(&remaining),
        "list-panes" | "lsp" => parse_list_panes(&remaining),
        "kill-pane" | "killp" => parse_kill_pane(&remaining),
        "display-message" | "display" => parse_display_message(&remaining),
        "select-pane" | "selectp" => parse_select_pane(&remaining),
        "set-option" | "set" => parse_set_option(&remaining),
        "select-layout" | "selectl" => parse_select_layout(&remaining),
        "resize-pane" | "resizep" => parse_resize_pane(&remaining),
        "has-session" | "has" => parse_has_session(&remaining),
        "new-session" | "new" => parse_new_session(&remaining),
        "new-window" | "neww" => parse_new_window(&remaining),
        "list-windows" | "lsw" => parse_list_windows(&remaining),
        "break-pane" | "breakp" => parse_break_pane(&remaining),
        "join-pane" | "joinp" => parse_join_pane(&remaining),
        "capture-pane" | "capturep" => parse_capture_pane(&remaining),
        other => Err(ShimError::parse(format!("unknown command: {}", other))),
    }
}

fn take_value<'a>(args: &[&'a str], i: &mut usize) -> Result<&'a str, ShimError> {
    *i += 1;
    args.get(*i)
        .copied()
        .ok_or_else(|| ShimError::parse("expected value after flag"))
}

fn parse_split_window<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut horizontal = false;
    let mut size = None;
    let mut print_info = false;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-h" => horizontal = true,
            "-v" => horizontal = false,
            "-l" => size = Some(take_value(args, &mut i)?),
            "-P" => print_info = true,
            "-F" => format = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::SplitWindow(SplitWindowArgs {
        target,
        horizontal,
        size,
        print_info,
        format,
    }))
}

fn parse_send_keys<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut keys = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => {
                target = Some(take_value(args, &mut i)?);
            }
            _ => {
                // All remaining args are keys
                keys.extend_from_slice(&args[i..]);
                break;
            }
        }
        i += 1;
    }

    Ok(TmuxCommand::SendKeys(SendKeysArgs { target, keys }))
}

fn parse_list_panes<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-F" => format = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::ListPanes(ListPanesArgs { target, format }))
}

fn parse_kill_pane<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "-t" {
            target = Some(take_value(args, &mut i)?);
        }
        i += 1;
    }

    Ok(TmuxCommand::KillPane(KillPaneArgs { target }))
}

fn parse_display_message<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut print = false;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-p" => print = true,
            arg if !arg.starts_with('-') => {
                format = Some(arg);
            }
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::DisplayMessage(DisplayMsgArgs {
        target,
        print,
        format,
    }))
}

fn parse_select_pane<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut style = None;
    let mut title = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-P" => style = Some(take_value(args, &mut i)?),
            "-T" => title = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::SelectPane(SelectPaneArgs {
        target,
        style,
        title,
    }))
}

fn parse_set_option<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut scope = OptionScope::Session;
    let mut target = None;
    let mut i = 0;
    let mut positional: Vec<&'a str> = Vec::new();

    while i < args.len() {
        match args[i] {
            "-p" => scope = OptionScope::Pane,
            "-w" => scope = OptionScope::Window,
            "-t" => target = Some(take_value(args, &mut i)?),
            arg if !arg.starts_with('-') => {
                positional.push(arg);
            }
            _ => {}
        }
        i += 1;
    }

    if positional.len() < 2 {
        return Err(ShimError::parse("set-option requires key and value"));
    }

    let key = positional[0];
    let value = positional[1..].join(" ");

    Ok(TmuxCommand::SetOption(SetOptionArgs {
        scope,
        target,
        key,
        value,
    }))
}

fn parse_select_layout<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut layout = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            arg if !arg.starts_with('-') => {
                layout = Some(arg);
            }
            _ => {}
        }
        i += 1;
    }

    let layout = layout.ok_or_else(|| ShimError::parse("select-layout requires a layout name"))?;

    Ok(TmuxCommand::SelectLayout(SelectLayoutArgs {
        target,
        layout,
    }))
}

fn parse_resize_pane<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut width = None;
    let mut height = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-x" => width = Some(take_value(args, &mut i)?),
            "-y" => height = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::ResizePane(ResizePaneArgs {
        target,
        width,
        height,
    }))
}

fn parse_has_session<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "-t" {
            target = Some(take_value(args, &mut i)?);
        }
        i += 1;
    }

    let target = target.ok_or_else(|| ShimError::parse("has-session requires -t target"))?;

    Ok(TmuxCommand::HasSession(HasSessionArgs { target }))
}

fn parse_new_session<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut detached = false;
    let mut session_name = None;
    let mut window_name = None;
    let mut print_info = false;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-d" => detached = true,
            "-s" => session_name = Some(take_value(args, &mut i)?),
            "-n" => window_name = Some(take_value(args, &mut i)?),
            "-P" => print_info = true,
            "-F" => format = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::NewSession(NewSessionArgs {
        detached,
        session_name,
        window_name,
        print_info,
        format,
    }))
}

fn parse_new_window<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut name = None;
    let mut print_info = false;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-n" => name = Some(take_value(args, &mut i)?),
            "-P" => print_info = true,
            "-F" => format = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::NewWindow(NewWindowArgs {
        target,
        name,
        print_info,
        format,
    }))
}

fn parse_list_windows<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-F" => format = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::ListWindows(ListWindowsArgs { target, format }))
}

fn parse_break_pane<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut detached = false;
    let mut source = None;
    let mut target = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-d" => detached = true,
            "-s" => source = Some(take_value(args, &mut i)?),
            "-t" => target = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::BreakPane(BreakPaneArgs {
        detached,
        source,
        target,
    }))
}

fn parse_join_pane<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut horizontal = false;
    let mut source = None;
    let mut target = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-h" => horizontal = true,
            "-s" => source = Some(take_value(args, &mut i)?),
            "-t" => target = Some(take_value(args, &mut i)?),
            _ => {}
        }
        i += 1;
    }

    Ok(TmuxCommand::JoinPane(JoinPaneArgs {
        horizontal,
        source,
        target,
    }))
}

fn parse_capture_pane<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut print = false;
    let mut start_line = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            "-p" => print = true,
            "-S" => {
                let val = take_value(args, &mut i)?;
                start_line = Some(val.parse::<i64>().map_err(|e| {
                    ShimError::parse(format!("invalid start line '{}': {}", val, e))
                })?);
            }
            other => {
                debug!(event = "shim.capture_pane.unknown_flag", flag = other);
            }
        }
        i += 1;
    }

    Ok(TmuxCommand::CapturePane(CapturePaneArgs {
        target,
        print,
        start_line,
    }))
}

#[cfg(test)]
mod tests {
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
}
