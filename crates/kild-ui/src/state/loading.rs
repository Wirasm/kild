/// Tracks in-progress async operations to prevent double-dispatch.
///
/// Two independent dimensions:
/// - Per-branch: tracks which branches have in-flight row operations (open/stop)
/// - Dialog: tracks whether a dialog submit (create/destroy) is in flight
#[derive(Clone, Debug, Default)]
pub struct LoadingState {
    loading_branches: std::collections::HashSet<String>,
    dialog: bool,
}

impl LoadingState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a branch as having an in-flight operation.
    pub fn set_branch(&mut self, branch: &str) {
        self.loading_branches.insert(branch.to_string());
    }

    /// Clear the in-flight operation for a branch.
    pub fn clear_branch(&mut self, branch: &str) {
        self.loading_branches.remove(branch);
    }

    /// Check if a branch has an in-flight operation.
    pub fn is_branch_loading(&self, branch: &str) -> bool {
        self.loading_branches.contains(branch)
    }

    /// Mark a dialog operation as in-flight.
    pub fn set_dialog(&mut self) {
        self.dialog = true;
    }

    /// Clear the dialog operation flag.
    pub fn clear_dialog(&mut self) {
        self.dialog = false;
    }

    /// Check if a dialog operation is in-flight.
    pub fn is_dialog(&self) -> bool {
        self.dialog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_query_branch() {
        let mut state = LoadingState::new();
        assert!(!state.is_branch_loading("branch-1"));

        state.set_branch("branch-1");
        assert!(state.is_branch_loading("branch-1"));
        assert!(!state.is_branch_loading("branch-2"));
    }

    #[test]
    fn test_clear_branch() {
        let mut state = LoadingState::new();
        state.set_branch("branch-1");

        state.clear_branch("branch-1");
        assert!(!state.is_branch_loading("branch-1"));
    }

    #[test]
    fn test_clear_branch_not_loading_is_noop() {
        let mut state = LoadingState::new();
        state.clear_branch("nonexistent");
        assert!(!state.is_branch_loading("nonexistent"));
    }

    #[test]
    fn test_multiple_branches_independent() {
        let mut state = LoadingState::new();
        state.set_branch("branch-1");
        state.set_branch("branch-2");

        assert!(state.is_branch_loading("branch-1"));
        assert!(state.is_branch_loading("branch-2"));

        state.clear_branch("branch-1");
        assert!(!state.is_branch_loading("branch-1"));
        assert!(state.is_branch_loading("branch-2"));
    }

    #[test]
    fn test_set_branch_overwrites() {
        let mut state = LoadingState::new();
        state.set_branch("branch-1");
        state.set_branch("branch-1");
        assert!(state.is_branch_loading("branch-1"));
    }

    #[test]
    fn test_dialog_loading() {
        let mut state = LoadingState::new();
        assert!(!state.is_dialog());

        state.set_dialog();
        assert!(state.is_dialog());

        state.clear_dialog();
        assert!(!state.is_dialog());
    }

    #[test]
    fn test_dialog_and_branch_independent() {
        let mut state = LoadingState::new();
        state.set_dialog();
        state.set_branch("branch-1");

        assert!(state.is_dialog());
        assert!(state.is_branch_loading("branch-1"));

        state.clear_dialog();
        assert!(!state.is_dialog());
        assert!(state.is_branch_loading("branch-1"));
    }
}
