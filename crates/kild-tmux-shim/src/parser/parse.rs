use tracing::debug;

use crate::errors::ShimError;

use super::*;

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
