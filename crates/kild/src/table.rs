use kild_core::Session;

pub struct TableFormatter {
    branch_width: usize,
    agent_width: usize,
    status_width: usize,
    created_width: usize,
    port_width: usize,
    process_width: usize,
    command_width: usize,
    note_width: usize,
}

impl TableFormatter {
    pub fn new(sessions: &[Session]) -> Self {
        let branch_width = sessions
            .iter()
            .map(|s| s.branch.len())
            .max()
            .unwrap_or(16)
            .clamp(6, 50); // Between "Branch" header min and reasonable terminal width max

        Self {
            branch_width,
            agent_width: 7,
            status_width: 7,
            created_width: 19,
            port_width: 11,
            process_width: 11,
            command_width: 20,
            note_width: 30,
        }
    }

    pub fn print_table(&self, sessions: &[Session]) {
        self.print_header();
        for session in sessions {
            self.print_row(session);
        }
        self.print_footer();
    }

    fn print_header(&self) {
        println!("{}", self.top_border());
        println!("{}", self.header_row());
        println!("{}", self.separator());
    }

    fn print_footer(&self) {
        println!("{}", self.bottom_border());
    }

    fn print_row(&self, session: &Session) {
        let port_range = format!("{}-{}", session.port_range_start, session.port_range_end);
        let process_status = {
            let mut running = 0;
            let mut errored = 0;
            for agent_proc in session.agents() {
                if let Some(pid) = agent_proc.process_id() {
                    match kild_core::process::is_process_running(pid) {
                        Ok(true) => running += 1,
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(
                                event = "cli.list_process_check_failed",
                                pid = pid,
                                agent = agent_proc.agent(),
                                session_branch = &session.branch,
                                error = %e
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
        };
        let note_display = session.note.as_deref().unwrap_or("");

        println!(
            "│ {:<width_branch$} │ {:<width_agent$} │ {:<width_status$} │ {:<width_created$} │ {:<width_port$} │ {:<width_process$} │ {:<width_command$} │ {:<width_note$} │",
            truncate(&session.branch, self.branch_width),
            truncate(
                &if session.agent_count() > 1 {
                    format!(
                        "{} (+{})",
                        session
                            .latest_agent()
                            .map_or(session.agent.as_str(), |a| a.agent()),
                        session.agent_count() - 1
                    )
                } else {
                    session.agent.clone()
                },
                self.agent_width
            ),
            format!("{:?}", session.status).to_lowercase(),
            truncate(&session.created_at, self.created_width),
            truncate(&port_range, self.port_width),
            truncate(&process_status, self.process_width),
            truncate(
                session.latest_agent().map_or("", |a| a.command()),
                self.command_width
            ),
            truncate(note_display, self.note_width),
            width_branch = self.branch_width,
            width_agent = self.agent_width,
            width_status = self.status_width,
            width_created = self.created_width,
            width_port = self.port_width,
            width_process = self.process_width,
            width_command = self.command_width,
            width_note = self.note_width,
        );
    }

    fn top_border(&self) -> String {
        format!(
            "┌{}┬{}┬{}┬{}┬{}┬{}┬{}┬{}┐",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.note_width + 2),
        )
    }

    fn header_row(&self) -> String {
        format!(
            "│ {:<width_branch$} │ {:<width_agent$} │ {:<width_status$} │ {:<width_created$} │ {:<width_port$} │ {:<width_process$} │ {:<width_command$} │ {:<width_note$} │",
            "Branch",
            "Agent",
            "Status",
            "Created",
            "Port Range",
            "Process",
            "Command",
            "Note",
            width_branch = self.branch_width,
            width_agent = self.agent_width,
            width_status = self.status_width,
            width_created = self.created_width,
            width_port = self.port_width,
            width_process = self.process_width,
            width_command = self.command_width,
            width_note = self.note_width,
        )
    }

    fn separator(&self) -> String {
        format!(
            "├{}┼{}┼{}┼{}┼{}┼{}┼{}┼{}┤",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.note_width + 2),
        )
    }

    fn bottom_border(&self) -> String {
        format!(
            "└{}┴{}┴{}┴{}┴{}┴{}┴{}┴{}┘",
            "─".repeat(self.branch_width + 2),
            "─".repeat(self.agent_width + 2),
            "─".repeat(self.status_width + 2),
            "─".repeat(self.created_width + 2),
            "─".repeat(self.port_width + 2),
            "─".repeat(self.process_width + 2),
            "─".repeat(self.command_width + 2),
            "─".repeat(self.note_width + 2),
        )
    }
}

/// Truncate a string to a maximum display width, adding "..." if truncated.
///
/// Uses character count (not byte count) to safely handle UTF-8 strings
/// including emoji and multi-byte characters.
pub fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        // Safely truncate at character boundaries, not byte boundaries
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{:<width$}", format!("{}...", truncated), width = max_len)
    }
}
