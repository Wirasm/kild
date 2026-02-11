use unicode_width::UnicodeWidthStr;

use kild_core::PrInfo;
use kild_core::Session;
use kild_core::sessions::types::AgentStatusInfo;

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
    note_width: usize,
}

impl TableFormatter {
    pub fn new(
        sessions: &[Session],
        statuses: &[Option<AgentStatusInfo>],
        pr_infos: &[Option<PrInfo>],
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
                session.agent.clone()
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
            note_width,
        }
    }

    pub fn print_table(
        &self,
        sessions: &[Session],
        statuses: &[Option<AgentStatusInfo>],
        pr_infos: &[Option<PrInfo>],
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
        status_info: Option<&AgentStatusInfo>,
        pr_info: Option<&PrInfo>,
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

        println!(
            "│ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │",
            pad(&session.branch, self.branch_width),
            pad(&agent_display, self.agent_width),
            pad(
                &format!("{:?}", session.status).to_lowercase(),
                self.status_width
            ),
            pad(&activity_display, self.activity_width),
            pad(&session.created_at, self.created_width),
            pad(&port_range, self.port_width),
            pad(&process_status, self.process_width),
            pad(&command, self.command_width),
            pad(&pr_display, self.pr_width),
            pad(note_display, self.note_width),
        );
    }

    fn top_border(&self) -> String {
        format!(
            "┌{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}┐",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.activity_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.pr_width + 2),
            "─".repeat(self.note_width + 2),
        )
    }

    fn header_row(&self) -> String {
        format!(
            "│ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │ {} │",
            pad("Branch", self.branch_width),
            pad("Agent", self.agent_width),
            pad("Status", self.status_width),
            pad("Activity", self.activity_width),
            pad("Created", self.created_width),
            pad("Port Range", self.port_width),
            pad("Process", self.process_width),
            pad("Command", self.command_width),
            pad("PR", self.pr_width),
            pad("Note", self.note_width),
        )
    }

    fn separator(&self) -> String {
        format!(
            "├{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}┤",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.activity_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.pr_width + 2),
            "─".repeat(self.note_width + 2),
        )
    }

    fn bottom_border(&self) -> String {
        format!(
            "└{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}┘",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.activity_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.pr_width + 2),
            "─".repeat(self.note_width + 2),
        )
    }
}

/// Compute the terminal display width of a string.
///
/// Wide characters (CJK, emoji) count as 2 columns.
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Pad a string to a minimum display width without truncating.
///
/// Uses Unicode display width to handle wide characters (CJK, emoji).
fn pad(s: &str, min_width: usize) -> String {
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
