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
    /// Shell command to execute in the new pane (after `--` or as trailing positional args).
    /// When empty, a login shell (`$SHELL`) is spawned instead.
    pub command: Vec<&'a str>,
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
