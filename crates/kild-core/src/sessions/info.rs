//! Session enrichment types and detection logic.
//!
//! Provides `SessionInfo`, which combines a `Session` with computed
//! process status, git status, and diff statistics. This is the enriched
//! view of a session used by UI and CLI consumers.

use std::path::Path;

use crate::git::get_diff_stats;
use crate::git::types::DiffStats;
use crate::process::is_process_running;
use crate::sessions::types::{GitStatus, ProcessStatus, Session};
use crate::terminal::is_terminal_window_open;

/// Enriched session data combining a `Session` with computed status fields.
///
/// Created via `SessionInfo::from_session()`, which runs process detection,
/// git status checks, and diff stat computation.
///
/// Status fields reflect state at construction time and become stale as
/// processes start/stop and files change. Refresh via `from_session()` or
/// targeted field updates as needed.
///
/// Invariant: `uncommitted_diff` is `Some` only when `git_status` is `Dirty`.
#[derive(Clone)]
pub struct SessionInfo {
    pub session: Session,
    pub process_status: ProcessStatus,
    pub git_status: GitStatus,
    pub uncommitted_diff: Option<DiffStats>,
}

impl SessionInfo {
    /// Create a `SessionInfo` by enriching a `Session` with computed status.
    ///
    /// Runs process detection, git status check, and diff stat computation.
    pub fn from_session(session: Session) -> Self {
        let process_status = determine_process_status(&session);

        let git_status = if session.worktree_path.exists() {
            check_git_status(&session.worktree_path)
        } else {
            GitStatus::Unknown
        };

        let uncommitted_diff = if git_status == GitStatus::Dirty {
            get_diff_stats(&session.worktree_path)
                .map_err(|e| {
                    tracing::warn!(
                        event = "core.session.diff_stats_failed",
                        path = %session.worktree_path.display(),
                        error = %e,
                        "Failed to compute diff stats"
                    );
                })
                .ok()
        } else {
            None
        };

        Self {
            session,
            process_status,
            git_status,
            uncommitted_diff,
        }
    }
}

/// Determine process status from session data.
///
/// Uses PID-based detection as primary method, falling back to window-based
/// detection for terminals like Ghostty where PID is unavailable.
///
/// Detection failures are logged as warnings and return:
/// - `ProcessStatus::Unknown` when PID or window check errors
/// - `ProcessStatus::Stopped` when no detection method available
pub fn determine_process_status(session: &Session) -> ProcessStatus {
    let mut any_running = false;
    let mut any_unknown = false;

    for agent_proc in session.agents() {
        // Try PID-based detection first
        if let Some(pid) = agent_proc.process_id() {
            match is_process_running(pid) {
                Ok(true) => {
                    any_running = true;
                    continue;
                }
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!(
                        event = "core.session.process_check_failed",
                        pid = pid,
                        agent = agent_proc.agent(),
                        branch = &session.branch,
                        error = %e
                    );
                    any_unknown = true;
                    continue;
                }
            }
        }

        // Fallback to window-based detection
        if let (Some(terminal_type), Some(window_id)) =
            (agent_proc.terminal_type(), agent_proc.terminal_window_id())
        {
            match is_terminal_window_open(terminal_type, window_id) {
                Ok(Some(true)) => {
                    any_running = true;
                    continue;
                }
                Ok(Some(false) | None) => continue,
                Err(e) => {
                    tracing::warn!(
                        event = "core.session.window_check_failed",
                        terminal_type = ?terminal_type,
                        window_id = window_id,
                        agent = agent_proc.agent(),
                        branch = &session.branch,
                        error = %e
                    );
                    any_unknown = true;
                    continue;
                }
            }
        }
    }

    if any_running {
        return ProcessStatus::Running;
    }
    if any_unknown {
        return ProcessStatus::Unknown;
    }
    ProcessStatus::Stopped
}

/// Check if a worktree has uncommitted changes using git2.
///
/// Returns `GitStatus::Dirty` if there are uncommitted changes,
/// `GitStatus::Clean` if the worktree is clean, or `GitStatus::Unknown`
/// if the status check failed.
fn check_git_status(worktree_path: &Path) -> GitStatus {
    let repo = match git2::Repository::open(worktree_path) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                event = "core.session.git_status_error",
                path = %worktree_path.display(),
                error = %e,
                "Failed to open repository for status check"
            );
            return GitStatus::Unknown;
        }
    };

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    opts.include_ignored(false);

    match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => {
            if statuses.is_empty() {
                GitStatus::Clean
            } else {
                GitStatus::Dirty
            }
        }
        Err(e) => {
            tracing::warn!(
                event = "core.session.git_status_failed",
                path = %worktree_path.display(),
                error = %e,
                "Failed to get git status"
            );
            GitStatus::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::types::SessionStatus;
    use std::path::PathBuf;

    fn make_session(worktree_path: PathBuf) -> Session {
        Session::new(
            "test-id".to_string(),
            "test-project".to_string(),
            "test-branch".to_string(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
            None,
            None,
            None,
        )
    }

    #[test]
    fn test_determine_process_status_no_pid() {
        let session = make_session(PathBuf::from("/tmp/nonexistent"));
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_determine_process_status_dead_pid() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.set_agents(vec![make_agent("claude", Some(999999))]); // Non-existent PID
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_determine_process_status_live_pid() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.set_agents(vec![make_agent("claude", Some(std::process::id()))]); // Current process
        assert_eq!(determine_process_status(&session), ProcessStatus::Running);
    }

    #[test]
    fn test_from_session_nonexistent_path() {
        let session = make_session(PathBuf::from("/tmp/nonexistent-test-path"));
        let info = SessionInfo::from_session(session);
        assert_eq!(info.process_status, ProcessStatus::Stopped);
        assert_eq!(info.git_status, GitStatus::Unknown);
    }

    #[test]
    fn test_check_git_status_clean_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();

        assert_eq!(check_git_status(path), GitStatus::Clean);
    }

    #[test]
    fn test_check_git_status_dirty_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "hello").unwrap();

        assert_eq!(check_git_status(path), GitStatus::Dirty);
    }

    #[test]
    fn test_check_git_status_non_git_directory() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        assert_eq!(check_git_status(temp_dir.path()), GitStatus::Unknown);
    }

    #[test]
    fn test_check_git_status_nonexistent_directory() {
        let path = Path::new("/nonexistent/path/that/does/not/exist");
        assert_eq!(check_git_status(path), GitStatus::Unknown);
    }

    fn make_agent(agent: &str, pid: Option<u32>) -> crate::sessions::types::AgentProcess {
        crate::sessions::types::AgentProcess::new(
            agent.to_string(),
            String::new(),
            pid,
            pid.map(|_| "test-process".to_string()),
            pid.map(|_| 1234567890),
            None,
            None,
            String::new(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_multi_agent_all_dead_returns_stopped() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.set_agents(vec![
            make_agent("claude", Some(999997)),
            make_agent("kiro", Some(999998)),
        ]);
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_multi_agent_one_alive_returns_running() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.set_agents(vec![
            make_agent("claude", Some(999997)),           // dead
            make_agent("kiro", Some(std::process::id())), // alive (self)
        ]);
        assert_eq!(determine_process_status(&session), ProcessStatus::Running);
    }

    #[test]
    fn test_multi_agent_no_pids_returns_stopped() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.set_agents(vec![make_agent("claude", None), make_agent("kiro", None)]);
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_multi_agent_empty_vec_returns_stopped() {
        let session = make_session(PathBuf::from("/tmp/nonexistent"));
        // Empty agents vec means no processes to check -> Stopped
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_multi_agent_mixed_pids_and_no_pids() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.set_agents(vec![
            make_agent("claude", Some(std::process::id())), // alive
            make_agent("kiro", None),                       // no PID
            make_agent("gemini", Some(999999)),             // dead
        ]);
        // Should return Running because at least one is alive
        assert_eq!(determine_process_status(&session), ProcessStatus::Running);
    }

    #[test]
    fn test_session_add_agent_appends() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        assert!(!session.has_agents());
        assert_eq!(session.agent_count(), 0);

        session.add_agent(make_agent("claude", Some(12345)));
        assert!(session.has_agents());
        assert_eq!(session.agent_count(), 1);
        assert_eq!(session.agents()[0].agent(), "claude");

        session.add_agent(make_agent("kiro", Some(67890)));
        assert_eq!(session.agent_count(), 2);
        assert_eq!(session.agents()[1].agent(), "kiro");
    }

    #[test]
    fn test_session_latest_agent() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        assert!(session.latest_agent().is_none());

        session.add_agent(make_agent("claude", None));
        assert_eq!(session.latest_agent().unwrap().agent(), "claude");

        session.add_agent(make_agent("kiro", None));
        assert_eq!(session.latest_agent().unwrap().agent(), "kiro");
    }

    #[test]
    fn test_session_clear_agents() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.add_agent(make_agent("claude", Some(12345)));
        session.add_agent(make_agent("kiro", Some(67890)));
        assert_eq!(session.agent_count(), 2);

        session.clear_agents();
        assert!(!session.has_agents());
        assert_eq!(session.agent_count(), 0);
        assert!(session.latest_agent().is_none());
    }

    #[test]
    fn test_from_session_dirty_repo_has_uncommitted_diff() {
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "line1\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();

        // Make it dirty
        std::fs::write(path.join("test.txt"), "line1\nline2\nline3\n").unwrap();

        let session = make_session(path.to_path_buf());
        let info = SessionInfo::from_session(session);

        assert_eq!(info.git_status, GitStatus::Dirty);
        assert!(info.uncommitted_diff.is_some());
        let stats = info.uncommitted_diff.unwrap();
        assert_eq!(stats.insertions, 2);
        assert_eq!(stats.files_changed, 1);
    }
}
