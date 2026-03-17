//! In-memory idle gate — deduplicates consecutive idle events per session.
//!
//! Replaces the file-based `.idle_sent` gate. When a session transitions to idle,
//! the gate is "armed" so subsequent idle events are suppressed until the next task
//! write clears it.

use std::collections::HashSet;

/// In-memory idle deduplication gate.
///
/// Tracks whether the first idle event for a session has already been forwarded
/// to the brain. Subsequent idle events are suppressed until `clear()` is called
/// (typically when a new task is injected).
pub struct IdleGate {
    armed: HashSet<String>,
}

impl Default for IdleGate {
    fn default() -> Self {
        Self::new()
    }
}

impl IdleGate {
    pub fn new() -> Self {
        Self {
            armed: HashSet::new(),
        }
    }

    /// Try to arm the gate for `branch`. Returns `true` if this is the first idle
    /// event (gate was not already armed) — meaning the caller should forward it.
    /// Returns `false` if the gate was already armed (duplicate idle, suppress).
    pub fn try_arm(&mut self, branch: &str) -> bool {
        self.armed.insert(branch.to_string())
    }

    /// Clear the gate for `branch`, allowing the next idle event to be forwarded.
    pub fn clear(&mut self, branch: &str) {
        self.armed.remove(branch);
    }

    /// Clear all gates (e.g. on daemon restart).
    #[cfg(test)]
    pub fn clear_all(&mut self) {
        self.armed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_idle_event_is_forwarded() {
        let mut gate = IdleGate::new();
        assert!(gate.try_arm("worker-a"), "First idle should pass through");
    }

    #[test]
    fn duplicate_idle_event_is_suppressed() {
        let mut gate = IdleGate::new();
        assert!(gate.try_arm("worker-a"));
        assert!(
            !gate.try_arm("worker-a"),
            "Second idle should be suppressed"
        );
        assert!(!gate.try_arm("worker-a"), "Third idle should be suppressed");
    }

    #[test]
    fn clear_resets_gate() {
        let mut gate = IdleGate::new();
        assert!(gate.try_arm("worker-a"));
        gate.clear("worker-a");
        assert!(
            gate.try_arm("worker-a"),
            "Idle after clear should pass through"
        );
    }

    #[test]
    fn gates_are_independent_per_branch() {
        let mut gate = IdleGate::new();
        assert!(gate.try_arm("worker-a"));
        assert!(
            gate.try_arm("worker-b"),
            "Different branch should have its own gate"
        );
        assert!(
            !gate.try_arm("worker-a"),
            "worker-a gate should still be armed"
        );
    }

    #[test]
    fn clear_all_resets_everything() {
        let mut gate = IdleGate::new();
        gate.try_arm("worker-a");
        gate.try_arm("worker-b");
        gate.clear_all();
        assert!(gate.try_arm("worker-a"));
        assert!(gate.try_arm("worker-b"));
    }
}
