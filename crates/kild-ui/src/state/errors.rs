// Allow dead_code — error types are consumed as operation flows are wired up.
#![allow(dead_code)]

/// Error from a kild operation, with the branch name for context.
#[derive(Clone, Debug)]
pub struct OperationError {
    pub branch: String,
    pub message: String,
}

/// Unified error tracking for kild operations.
///
/// Consolidates per-branch errors (open, stop, editor, focus) and bulk operation
/// errors into a single struct with a consistent API.
#[derive(Clone, Debug, Default)]

pub struct OperationErrors {
    /// Per-branch errors (keyed by branch name).
    by_branch: std::collections::HashMap<String, OperationError>,
    /// Bulk operation errors (e.g., "open all" failures).
    bulk: Vec<OperationError>,
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

    /// Set bulk errors (replaces existing).
    pub fn set_bulk(&mut self, errors: Vec<OperationError>) {
        self.bulk = errors;
    }

    /// Get bulk operation errors.
    pub fn bulk_errors(&self) -> &[OperationError] {
        &self.bulk
    }

    /// Check if there are any bulk errors.
    pub fn has_bulk_errors(&self) -> bool {
        !self.bulk.is_empty()
    }

    /// Clear all bulk operation errors.
    pub fn clear_bulk(&mut self) {
        self.bulk.clear();
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

    #[test]
    fn test_operation_errors_bulk() {
        let mut errors = OperationErrors::new();

        errors.set_bulk(vec![
            OperationError {
                branch: "branch-1".to_string(),
                message: "error 1".to_string(),
            },
            OperationError {
                branch: "branch-2".to_string(),
                message: "error 2".to_string(),
            },
        ]);

        assert!(errors.has_bulk_errors());
        assert_eq!(errors.bulk_errors().len(), 2);

        errors.clear_bulk();
        assert!(!errors.has_bulk_errors());
    }
}
