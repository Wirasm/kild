use unicode_width::UnicodeWidthStr;

use kild_core::PullRequest;
use kild_core::Session;
use kild_core::sessions::types::AgentStatusRecord;

use crate::color;

pub struct TableFormatter {
    branch_width: usize,
    agent_width: usize,
    status_width: usize,
    activity_width: usize,
    created_width: usize,
    port_width: usize,
    process_width: usize,
    command_width: usize,
    pr_width: usize,
    issue_width: usize,
    show_issue: bool,
    note_width: usize,
}

impl TableFormatter {
    pub fn new(
        sessions: &[Session],
        statuses: &[Option<AgentStatusRecord>],
        pr_infos: &[Option<PullRequest>],
    ) -> Self {
        // Minimum widths = header label lengths
        let mut branch_width = "Branch".len();
        let mut agent_width = "Agent".len();
        let mut status_width = "Status".len();
        let mut activity_width = "Activity".len();
        let mut created_width = "Created".len();
        let mut port_width = "Port Range".len();
        let mut process_width = "Process".len();
        let mut command_width = "Command".len();
        let mut pr_width = "PR".len();
        let mut note_width = "Note".len();

        let show_issue = sessions.iter().any(|s| s.issue.is_some());
        let mut issue_width = if show_issue { "Issue".len() } else { 0 };

        for (i, session) in sessions.iter().enumerate() {
            branch_width = branch_width.max(display_width(&session.branch));

            let agent_display = if session.agent_count() > 1 {
                format!(
                    "{} (+{})",
                    session
                        .latest_agent()
                        .map_or(session.agent.as_str(), |a| a.agent()),
                    session.agent_count() - 1
                )
            } else {
                session
                    .latest_agent()
                    .map_or(session.agent.clone(), |a| a.agent().to_string())
            };
            agent_width = agent_width.max(display_width(&agent_display));

            let status_str = format!("{:?}", session.status).to_lowercase();
            status_width = status_width.max(display_width(&status_str));

            let activity = statuses
                .get(i)
                .and_then(|s| s.as_ref())
                .map_or("-".to_string(), |info| info.status.to_string());
            activity_width = activity_width.max(display_width(&activity));

            created_width = created_width.max(display_width(&session.created_at));

            let port_range = format!("{}-{}", session.port_range_start, session.port_range_end);
            port_width = port_width.max(display_width(&port_range));

            let process_str = Self::format_process_status(session);
            process_width = process_width.max(display_width(&process_str));

            let command = session.latest_agent().map_or("", |a| a.command());
            command_width = command_width.max(display_width(command));

            let pr_display =
                pr_infos
                    .get(i)
                    .and_then(|p| p.as_ref())
                    .map_or("-".to_string(), |pr| match pr.state {
                        kild_core::PrState::Merged => "Merged".to_string(),
                        _ => format!("PR #{}", pr.number),
                    });
            pr_width = pr_width.max(display_width(&pr_display));

            if show_issue {
                let issue_display = session.issue.map_or(String::new(), |n| format!("#{}", n));
                issue_width = issue_width.max(display_width(&issue_display));
            }

            let note = session.note.as_deref().unwrap_or("");
            note_width = note_width.max(display_width(note));
        }

        Self {
            branch_width,
            agent_width,
            status_width,
            activity_width,
            created_width,
            port_width,
            process_width,
            command_width,
            pr_width,
            issue_width,
            show_issue,
            note_width,
        }
    }

    pub fn print_table(
        &self,
        sessions: &[Session],
        statuses: &[Option<AgentStatusRecord>],
        pr_infos: &[Option<PullRequest>],
    ) {
        self.print_header();
        for (i, session) in sessions.iter().enumerate() {
            let status_info = statuses.get(i).and_then(|s| s.as_ref());
            let pr_info = pr_infos.get(i).and_then(|p| p.as_ref());
            self.print_row(session, status_info, pr_info);
        }
        self.print_footer();
    }

    fn format_process_status(session: &Session) -> String {
        let mut running = 0;
        let mut errored = 0;
        for agent_proc in session.agents() {
            if let Some(pid) = agent_proc.process_id() {
                match kild_core::process::is_process_running(pid) {
                    Ok(true) => running += 1,
                    Ok(false) => {}
                    Err(_) => errored += 1,
                }
            } else if let Some(daemon_sid) = agent_proc.daemon_session_id() {
                match kild_core::daemon::client::get_session_status(daemon_sid) {
                    Ok(Some(
                        kild_protocol::SessionStatus::Running
                        | kild_protocol::SessionStatus::Creating,
                    )) => running += 1,
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(
                            event = "cli.list.daemon_check_failed",
                            daemon_session_id = daemon_sid,
                            error = %e,
                        );
                        errored += 1;
                    }
                }
            }
        }
        let total = session.agent_count();
        if total == 0 {
            "No PID".to_string()
        } else if errored > 0 {
            format!("{}run,{}err/{}", running, errored, total)
        } else {
            format!("Run({}/{})", running, total)
        }
    }

    fn issue_border_segment(&self) -> String {
        if self.show_issue {
            format!("┬{}", "─".repeat(self.issue_width + 2))
        } else {
            String::new()
        }
    }

    fn issue_separator_segment(&self) -> String {
        if self.show_issue {
            format!("┼{}", "─".repeat(self.issue_width + 2))
        } else {
            String::new()
        }
    }

    fn issue_bottom_segment(&self) -> String {
        if self.show_issue {
            format!("┴{}", "─".repeat(self.issue_width + 2))
        } else {
            String::new()
        }
    }

    fn print_header(&self) {
        println!("{}", self.top_border());
        println!("{}", self.header_row());
        println!("{}", self.separator());
    }

    fn print_footer(&self) {
        println!("{}", self.bottom_border());
    }

    fn print_row(
        &self,
        session: &Session,
        status_info: Option<&AgentStatusRecord>,
        pr_info: Option<&PullRequest>,
    ) {
        let port_range = format!("{}-{}", session.port_range_start, session.port_range_end);
        let process_status = Self::format_process_status(session);
        let note_display = session.note.as_deref().unwrap_or("");
        let activity_display =
            status_info.map_or_else(|| "-".to_string(), |i| i.status.to_string());
        let pr_display = pr_info.map_or_else(
            || "-".to_string(),
            |pr| match pr.state {
                kild_core::PrState::Merged => "Merged".to_string(),
                _ => format!("PR #{}", pr.number),
            },
        );
        let agent_display = if session.agent_count() > 1 {
            format!(
                "{} (+{})",
                session
                    .latest_agent()
                    .map_or(session.agent.as_str(), |a| a.agent()),
                session.agent_count() - 1
            )
        } else {
            session.agent.clone()
        };
        let command = session
            .latest_agent()
            .map_or("".to_string(), |a| a.command().to_string());

        let status_str = format!("{:?}", session.status).to_lowercase();
        let sep = color::muted("│");

        let issue_cell = if self.show_issue {
            let issue_display = session.issue.map_or(String::new(), |n| format!("#{}", n));
            format!(" {sep} {}", pad(&issue_display, self.issue_width))
        } else {
            String::new()
        };

        println!(
            "{sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {}{} {sep} {} {sep}",
            color::ice(&pad(&session.branch, self.branch_width)),
            color::kiri(&pad(&agent_display, self.agent_width)),
            color::status(&pad(&status_str, self.status_width)),
            color::activity(&pad(&activity_display, self.activity_width)),
            pad(&session.created_at, self.created_width),
            pad(&port_range, self.port_width),
            pad(&process_status, self.process_width),
            pad(&command, self.command_width),
            pad(&pr_display, self.pr_width),
            issue_cell,
            pad(note_display, self.note_width),
        );
    }

    fn top_border(&self) -> String {
        let issue = self.issue_border_segment();
        color::muted(&format!(
            "┌{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}{}┬{}┐",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.activity_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.pr_width + 2),
            issue,
            "─".repeat(self.note_width + 2),
        ))
    }

    fn header_row(&self) -> String {
        let sep = color::muted("│");
        let issue_cell = if self.show_issue {
            format!(" {sep} {}", color::bold(&pad("Issue", self.issue_width)))
        } else {
            String::new()
        };
        format!(
            "{sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {} {sep} {}{} {sep} {} {sep}",
            color::bold(&pad("Branch", self.branch_width)),
            color::bold(&pad("Agent", self.agent_width)),
            color::bold(&pad("Status", self.status_width)),
            color::bold(&pad("Activity", self.activity_width)),
            color::bold(&pad("Created", self.created_width)),
            color::bold(&pad("Port Range", self.port_width)),
            color::bold(&pad("Process", self.process_width)),
            color::bold(&pad("Command", self.command_width)),
            color::bold(&pad("PR", self.pr_width)),
            issue_cell,
            color::bold(&pad("Note", self.note_width)),
        )
    }

    fn separator(&self) -> String {
        let issue = self.issue_separator_segment();
        color::muted(&format!(
            "├{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}{}┼{}┤",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.activity_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.pr_width + 2),
            issue,
            "─".repeat(self.note_width + 2),
        ))
    }

    fn bottom_border(&self) -> String {
        let issue = self.issue_bottom_segment();
        color::muted(&format!(
            "└{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}{}┴{}┘",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.activity_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.pr_width + 2),
            issue,
            "─".repeat(self.note_width + 2),
        ))
    }
}

/// Compute the terminal display width of a string.
///
/// Wide characters (CJK, emoji) count as 2 columns.
pub(crate) fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Pad a string to a minimum display width without truncating.
///
/// Uses Unicode display width to handle wide characters (CJK, emoji).
pub(crate) fn pad(s: &str, min_width: usize) -> String {
    let width = display_width(s);
    if width >= min_width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(min_width - width))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_shorter_than_width() {
        assert_eq!(pad("hi", 5), "hi   ");
    }

    #[test]
    fn test_pad_exact_width() {
        assert_eq!(pad("hello", 5), "hello");
    }

    #[test]
    fn test_pad_longer_than_width() {
        // Never truncates
        assert_eq!(pad("hello world", 5), "hello world");
    }

    #[test]
    fn test_display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
    }

    #[test]
    fn test_display_width_cjk() {
        // CJK characters are 2 cells wide
        assert_eq!(display_width("日本"), 4);
    }

    #[test]
    fn test_display_width_emoji() {
        // Emoji are typically 2 cells wide
        assert_eq!(display_width("🚀"), 2);
    }

    #[test]
    fn test_display_width_mixed() {
        // "Hello " = 6, "世界" = 4, " " = 1, "🌍" = 2 => 13
        assert_eq!(display_width("Hello 世界 🌍"), 13);
    }

    #[test]
    fn test_pad_with_wide_chars() {
        // "日本" is 4 display width, pad to 6 => 2 spaces added
        assert_eq!(pad("日本", 6), "日本  ");
    }
}
