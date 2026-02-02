/// Type of operation in progress.
#[derive(Clone, Debug)]
pub enum Operation {
    Opening,
    Stopping,
}

/// Tracks in-progress async operations to prevent double-dispatch.
///
/// Three independent dimensions:
/// - Per-branch: tracks which branches have in-flight row operations (open/stop)
/// - Bulk: tracks whether a bulk operation (open-all/stop-all) is in flight
/// - Dialog: tracks whether a dialog submit (create/destroy) is in flight
#[derive(Clone, Debug, Default)]
pub struct LoadingState {
    by_branch: std::collections::HashMap<String, Operation>,
    bulk: bool,
    dialog: bool,
}

impl LoadingState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a branch as having an in-flight operation.
    pub fn set_branch(&mut self, branch: &str, op: Operation) {
        self.by_branch.insert(branch.to_string(), op);
    }

    /// Clear the in-flight operation for a branch.
    pub fn clear_branch(&mut self, branch: &str) {
        self.by_branch.remove(branch);
    }

    /// Check if a branch has an in-flight operation.
    pub fn is_branch_loading(&self, branch: &str) -> bool {
        self.by_branch.contains_key(branch)
    }

    /// Mark a bulk operation as in-flight.
    pub fn set_bulk(&mut self) {
        self.bulk = true;
    }

    /// Clear the bulk operation flag.
    pub fn clear_bulk(&mut self) {
        self.bulk = false;
    }

    /// Check if a bulk operation is in-flight.
    pub fn is_bulk(&self) -> bool {
        self.bulk
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

        state.set_branch("branch-1", Operation::Opening);
        assert!(state.is_branch_loading("branch-1"));
        assert!(!state.is_branch_loading("branch-2"));
    }

    #[test]
    fn test_clear_branch() {
        let mut state = LoadingState::new();
        state.set_branch("branch-1", Operation::Opening);

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
        state.set_branch("branch-1", Operation::Opening);
        state.set_branch("branch-2", Operation::Stopping);

        assert!(state.is_branch_loading("branch-1"));
        assert!(state.is_branch_loading("branch-2"));

        state.clear_branch("branch-1");
        assert!(!state.is_branch_loading("branch-1"));
        assert!(state.is_branch_loading("branch-2"));
    }

    #[test]
    fn test_set_branch_idempotent() {
        let mut state = LoadingState::new();
        state.set_branch("branch-1", Operation::Opening);
        state.set_branch("branch-1", Operation::Stopping);
        assert!(state.is_branch_loading("branch-1"));
    }

    #[test]
    fn test_bulk_loading() {
        let mut state = LoadingState::new();
        assert!(!state.is_bulk());

        state.set_bulk();
        assert!(state.is_bulk());

        state.clear_bulk();
        assert!(!state.is_bulk());
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
    fn test_bulk_and_branch_independent() {
        let mut state = LoadingState::new();
        state.set_bulk();
        state.set_branch("branch-1", Operation::Opening);

        assert!(state.is_bulk());
        assert!(state.is_branch_loading("branch-1"));

        state.clear_bulk();
        assert!(!state.is_bulk());
        assert!(state.is_branch_loading("branch-1"));
    }

    #[test]
    fn test_dialog_and_branch_independent() {
        let mut state = LoadingState::new();
        state.set_dialog();
        state.set_branch("branch-1", Operation::Stopping);

        assert!(state.is_dialog());
        assert!(state.is_branch_loading("branch-1"));

        state.clear_dialog();
        assert!(!state.is_dialog());
        assert!(state.is_branch_loading("branch-1"));
    }
}
