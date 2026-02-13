/// Error from a kild operation, with the branch name for context.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct OperationError {
    pub branch: String,
    pub message: String,
}

/// Per-branch error tracking for kild operations.
#[derive(Clone, Debug, Default)]
pub struct OperationErrors {
    /// Per-branch errors (keyed by branch name).
    by_branch: std::collections::HashMap<String, OperationError>,
}

impl OperationErrors {
    /// Create a new empty error collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an error for a specific branch (replaces any existing error).
    pub fn set(&mut self, branch: &str, error: OperationError) {
        self.by_branch.insert(branch.to_string(), error);
    }

    /// Get the error for a specific branch, if any.
    pub fn get(&self, branch: &str) -> Option<&OperationError> {
        self.by_branch.get(branch)
    }

    /// Clear the error for a specific branch.
    pub fn clear(&mut self, branch: &str) {
        self.by_branch.remove(branch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_errors_set_and_get() {
        let mut errors = OperationErrors::new();

        errors.set(
            "branch-1",
            OperationError {
                branch: "branch-1".to_string(),
                message: "error 1".to_string(),
            },
        );

        assert!(errors.get("branch-1").is_some());
        assert_eq!(errors.get("branch-1").unwrap().message, "error 1");
        assert!(errors.get("branch-2").is_none());
    }

    #[test]
    fn test_operation_errors_clear() {
        let mut errors = OperationErrors::new();

        errors.set(
            "branch-1",
            OperationError {
                branch: "branch-1".to_string(),
                message: "error 1".to_string(),
            },
        );
        errors.clear("branch-1");

        assert!(errors.get("branch-1").is_none());
    }
}
