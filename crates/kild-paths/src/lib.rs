use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("home directory not found — set $HOME environment variable")]
    HomeNotFound,
}

/// Centralized path construction for the `~/.kild/` directory layout.
///
/// Single source of truth for every path under `~/.kild/`. Use `resolve()` in
/// production code and `from_dir()` in tests.
#[derive(Debug, Clone)]
pub struct KildPaths {
    kild_dir: PathBuf,
}

impl KildPaths {
    /// Resolve paths from the user's home directory (`~/.kild`).
    pub fn resolve() -> Result<Self, PathError> {
        let home = dirs::home_dir().ok_or(PathError::HomeNotFound)?;
        Ok(Self {
            kild_dir: home.join(".kild"),
        })
    }

    /// Create paths from an explicit base directory. Use in tests.
    pub fn from_dir(kild_dir: PathBuf) -> Self {
        Self { kild_dir }
    }

    /// The base `~/.kild` directory.
    pub fn kild_dir(&self) -> &Path {
        &self.kild_dir
    }

    // --- Top-level subdirectories ---

    pub fn sessions_dir(&self) -> PathBuf {
        self.kild_dir.join("sessions")
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.kild_dir.join("worktrees")
    }

    pub fn pids_dir(&self) -> PathBuf {
        self.kild_dir.join("pids")
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.kild_dir.join("bin")
    }

    pub fn hooks_dir(&self) -> PathBuf {
        self.kild_dir.join("hooks")
    }

    pub fn shim_dir(&self) -> PathBuf {
        self.kild_dir.join("shim")
    }

    pub fn health_history_dir(&self) -> PathBuf {
        self.kild_dir.join("health_history")
    }

    // --- Fleet paths ---

    pub fn fleet_dir(&self) -> PathBuf {
        self.kild_dir.join("fleet")
    }

    pub fn fleet_project_dir(&self, project_id: &str) -> PathBuf {
        self.fleet_dir().join(project_id)
    }

    pub fn fleet_dropbox_dir(&self, project_id: &str, branch: &str) -> PathBuf {
        let safe_branch = branch.replace('/', "_");
        self.fleet_project_dir(project_id).join(safe_branch)
    }

    // --- Top-level files ---

    pub fn daemon_socket(&self) -> PathBuf {
        self.kild_dir.join("daemon.sock")
    }

    pub fn tls_cert_path(&self) -> PathBuf {
        self.kild_dir.join("certs").join("daemon.crt")
    }

    pub fn tls_key_path(&self) -> PathBuf {
        self.kild_dir.join("certs").join("daemon.key")
    }

    pub fn daemon_pid_file(&self) -> PathBuf {
        self.kild_dir.join("daemon.pid")
    }

    pub fn daemon_bin_file(&self) -> PathBuf {
        self.kild_dir.join("daemon.bin")
    }

    pub fn projects_file(&self) -> PathBuf {
        self.kild_dir.join("projects.json")
    }

    pub fn user_config(&self) -> PathBuf {
        self.kild_dir.join("config.toml")
    }

    pub fn user_keybindings(&self) -> PathBuf {
        self.kild_dir.join("keybindings.toml")
    }

    // --- Parameterized paths ---

    pub fn shim_session_dir(&self, session_id: &str) -> PathBuf {
        self.shim_dir().join(session_id)
    }

    pub fn shim_panes_file(&self, session_id: &str) -> PathBuf {
        self.shim_session_dir(session_id).join("panes.json")
    }

    pub fn shim_lock_file(&self, session_id: &str) -> PathBuf {
        self.shim_session_dir(session_id).join("panes.lock")
    }

    pub fn shim_zdotdir(&self, session_id: &str) -> PathBuf {
        self.shim_session_dir(session_id).join("zdotdir")
    }

    pub fn tmux_shim_binary(&self) -> PathBuf {
        self.bin_dir().join("tmux")
    }

    pub fn codex_notify_hook(&self) -> PathBuf {
        self.hooks_dir().join("codex-notify")
    }

    pub fn claude_status_hook(&self) -> PathBuf {
        self.hooks_dir().join("claude-status")
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        let safe_id = session_id.replace('/', "_");
        self.sessions_dir().join(safe_id)
    }

    pub fn session_file(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("kild.json")
    }

    pub fn session_status_file(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("status")
    }

    pub fn session_pr_file(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("pr")
    }

    pub fn pid_file(&self, session_id: &str) -> PathBuf {
        let safe_id = session_id.replace('/', "-");
        self.pids_dir().join(format!("{safe_id}.pid"))
    }

    // --- Static helpers (no self) ---

    /// Project-level config: `<project_root>/.kild/config.toml`.
    pub fn project_config(project_root: &Path) -> PathBuf {
        project_root.join(".kild").join("config.toml")
    }

    /// Project-level keybindings: `<project_root>/.kild/keybindings.toml`.
    pub fn project_keybindings(project_root: &Path) -> PathBuf {
        project_root.join(".kild").join("keybindings.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths() -> KildPaths {
        KildPaths::from_dir(PathBuf::from("/home/user/.kild"))
    }

    #[test]
    fn test_resolve_returns_ok_when_home_set() {
        // HOME is set in CI and dev environments
        let result = KildPaths::resolve();
        assert!(result.is_ok());
        let paths = result.unwrap();
        assert!(paths.kild_dir().to_string_lossy().contains(".kild"));
    }

    #[test]
    fn test_from_dir() {
        let paths = KildPaths::from_dir(PathBuf::from("/tmp/test-kild"));
        assert_eq!(paths.kild_dir(), Path::new("/tmp/test-kild"));
    }

    #[test]
    fn test_sessions_dir() {
        assert_eq!(
            test_paths().sessions_dir(),
            PathBuf::from("/home/user/.kild/sessions")
        );
    }

    #[test]
    fn test_worktrees_dir() {
        assert_eq!(
            test_paths().worktrees_dir(),
            PathBuf::from("/home/user/.kild/worktrees")
        );
    }

    #[test]
    fn test_pids_dir() {
        assert_eq!(
            test_paths().pids_dir(),
            PathBuf::from("/home/user/.kild/pids")
        );
    }

    #[test]
    fn test_bin_dir() {
        assert_eq!(
            test_paths().bin_dir(),
            PathBuf::from("/home/user/.kild/bin")
        );
    }

    #[test]
    fn test_hooks_dir() {
        assert_eq!(
            test_paths().hooks_dir(),
            PathBuf::from("/home/user/.kild/hooks")
        );
    }

    #[test]
    fn test_shim_dir() {
        assert_eq!(
            test_paths().shim_dir(),
            PathBuf::from("/home/user/.kild/shim")
        );
    }

    #[test]
    fn test_health_history_dir() {
        assert_eq!(
            test_paths().health_history_dir(),
            PathBuf::from("/home/user/.kild/health_history")
        );
    }

    #[test]
    fn test_daemon_socket() {
        assert_eq!(
            test_paths().daemon_socket(),
            PathBuf::from("/home/user/.kild/daemon.sock")
        );
    }

    #[test]
    fn test_tls_cert_path() {
        assert_eq!(
            test_paths().tls_cert_path(),
            PathBuf::from("/home/user/.kild/certs/daemon.crt")
        );
    }

    #[test]
    fn test_tls_key_path() {
        assert_eq!(
            test_paths().tls_key_path(),
            PathBuf::from("/home/user/.kild/certs/daemon.key")
        );
    }

    #[test]
    fn test_daemon_pid_file() {
        assert_eq!(
            test_paths().daemon_pid_file(),
            PathBuf::from("/home/user/.kild/daemon.pid")
        );
    }

    #[test]
    fn test_daemon_bin_file() {
        assert_eq!(
            test_paths().daemon_bin_file(),
            PathBuf::from("/home/user/.kild/daemon.bin")
        );
    }

    #[test]
    fn test_projects_file() {
        assert_eq!(
            test_paths().projects_file(),
            PathBuf::from("/home/user/.kild/projects.json")
        );
    }

    #[test]
    fn test_user_config() {
        assert_eq!(
            test_paths().user_config(),
            PathBuf::from("/home/user/.kild/config.toml")
        );
    }

    #[test]
    fn test_shim_session_dir() {
        assert_eq!(
            test_paths().shim_session_dir("my-session"),
            PathBuf::from("/home/user/.kild/shim/my-session")
        );
    }

    #[test]
    fn test_shim_panes_file() {
        assert_eq!(
            test_paths().shim_panes_file("my-session"),
            PathBuf::from("/home/user/.kild/shim/my-session/panes.json")
        );
    }

    #[test]
    fn test_shim_lock_file() {
        assert_eq!(
            test_paths().shim_lock_file("my-session"),
            PathBuf::from("/home/user/.kild/shim/my-session/panes.lock")
        );
    }

    #[test]
    fn test_shim_zdotdir() {
        assert_eq!(
            test_paths().shim_zdotdir("my-session"),
            PathBuf::from("/home/user/.kild/shim/my-session/zdotdir")
        );
    }

    #[test]
    fn test_tmux_shim_binary() {
        assert_eq!(
            test_paths().tmux_shim_binary(),
            PathBuf::from("/home/user/.kild/bin/tmux")
        );
    }

    #[test]
    fn test_codex_notify_hook() {
        assert_eq!(
            test_paths().codex_notify_hook(),
            PathBuf::from("/home/user/.kild/hooks/codex-notify")
        );
    }

    #[test]
    fn test_claude_status_hook() {
        assert_eq!(
            test_paths().claude_status_hook(),
            PathBuf::from("/home/user/.kild/hooks/claude-status")
        );
    }

    #[test]
    fn test_session_dir() {
        assert_eq!(
            test_paths().session_dir("proj_branch"),
            PathBuf::from("/home/user/.kild/sessions/proj_branch")
        );
    }

    #[test]
    fn test_session_dir_sanitizes_slashes() {
        assert_eq!(
            test_paths().session_dir("project/branch"),
            PathBuf::from("/home/user/.kild/sessions/project_branch")
        );
    }

    #[test]
    fn test_session_dir_multiple_slashes() {
        assert_eq!(
            test_paths().session_dir("a/b/c"),
            PathBuf::from("/home/user/.kild/sessions/a_b_c")
        );
    }

    #[test]
    fn test_session_file() {
        assert_eq!(
            test_paths().session_file("proj_branch"),
            PathBuf::from("/home/user/.kild/sessions/proj_branch/kild.json")
        );
    }

    #[test]
    fn test_session_file_sanitizes_slashes() {
        assert_eq!(
            test_paths().session_file("project/branch"),
            PathBuf::from("/home/user/.kild/sessions/project_branch/kild.json")
        );
    }

    #[test]
    fn test_session_status_file() {
        assert_eq!(
            test_paths().session_status_file("proj_branch"),
            PathBuf::from("/home/user/.kild/sessions/proj_branch/status")
        );
    }

    #[test]
    fn test_session_status_file_sanitizes_slashes() {
        assert_eq!(
            test_paths().session_status_file("project/branch"),
            PathBuf::from("/home/user/.kild/sessions/project_branch/status")
        );
    }

    #[test]
    fn test_session_pr_file() {
        assert_eq!(
            test_paths().session_pr_file("proj_branch"),
            PathBuf::from("/home/user/.kild/sessions/proj_branch/pr")
        );
    }

    #[test]
    fn test_session_pr_file_sanitizes_slashes() {
        assert_eq!(
            test_paths().session_pr_file("project/branch"),
            PathBuf::from("/home/user/.kild/sessions/project_branch/pr")
        );
    }

    #[test]
    fn test_pid_file_simple() {
        assert_eq!(
            test_paths().pid_file("abc123"),
            PathBuf::from("/home/user/.kild/pids/abc123.pid")
        );
    }

    #[test]
    fn test_pid_file_sanitizes_slashes() {
        assert_eq!(
            test_paths().pid_file("project/branch"),
            PathBuf::from("/home/user/.kild/pids/project-branch.pid")
        );
    }

    #[test]
    fn test_pid_file_multiple_slashes() {
        assert_eq!(
            test_paths().pid_file("a//b///c"),
            PathBuf::from("/home/user/.kild/pids/a--b---c.pid")
        );
    }

    #[test]
    fn test_pid_file_leading_trailing_slashes() {
        assert_eq!(
            test_paths().pid_file("/branch/"),
            PathBuf::from("/home/user/.kild/pids/-branch-.pid")
        );
    }

    #[test]
    fn test_pid_file_empty_session_id() {
        assert_eq!(
            test_paths().pid_file(""),
            PathBuf::from("/home/user/.kild/pids/.pid")
        );
    }

    #[test]
    fn test_path_error_message() {
        let err = PathError::HomeNotFound;
        let msg = err.to_string();
        assert!(msg.contains("home directory not found"));
        assert!(msg.contains("$HOME"));
    }

    #[test]
    fn test_project_config() {
        assert_eq!(
            KildPaths::project_config(Path::new("/my/project")),
            PathBuf::from("/my/project/.kild/config.toml")
        );
    }

    #[test]
    fn test_user_keybindings() {
        assert_eq!(
            test_paths().user_keybindings(),
            PathBuf::from("/home/user/.kild/keybindings.toml")
        );
    }

    #[test]
    fn test_project_keybindings() {
        assert_eq!(
            KildPaths::project_keybindings(Path::new("/my/project")),
            PathBuf::from("/my/project/.kild/keybindings.toml")
        );
    }

    #[test]
    fn test_fleet_dir() {
        assert_eq!(
            test_paths().fleet_dir(),
            PathBuf::from("/home/user/.kild/fleet")
        );
    }

    #[test]
    fn test_fleet_project_dir() {
        assert_eq!(
            test_paths().fleet_project_dir("abc123"),
            PathBuf::from("/home/user/.kild/fleet/abc123")
        );
    }

    #[test]
    fn test_fleet_dropbox_dir() {
        assert_eq!(
            test_paths().fleet_dropbox_dir("abc123", "my-branch"),
            PathBuf::from("/home/user/.kild/fleet/abc123/my-branch")
        );
    }

    #[test]
    fn test_fleet_dropbox_dir_sanitizes_slashes() {
        assert_eq!(
            test_paths().fleet_dropbox_dir("abc123", "feature/auth"),
            PathBuf::from("/home/user/.kild/fleet/abc123/feature_auth")
        );
    }

    #[test]
    fn test_fleet_dropbox_dir_multiple_slashes() {
        assert_eq!(
            test_paths().fleet_dropbox_dir("abc123", "a/b/c"),
            PathBuf::from("/home/user/.kild/fleet/abc123/a_b_c")
        );
    }
}
