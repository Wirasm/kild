use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResourceType {
    OrphanedBranch,
    OrphanedWorktree,
    StaleSession,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrphanedResource {
    pub resource_type: ResourceType,
    pub path: PathBuf,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CleanupSummary {
    pub orphaned_branches: Vec<String>,
    pub orphaned_worktrees: Vec<PathBuf>,
    pub stale_sessions: Vec<String>,
    pub total_cleaned: usize,
}

impl OrphanedResource {
    pub fn new(
        resource_type: ResourceType,
        path: PathBuf,
        name: String,
        description: String,
    ) -> Self {
        Self {
            resource_type,
            path,
            name,
            description,
        }
    }
}

impl CleanupSummary {
    pub fn new() -> Self {
        Self {
            orphaned_branches: Vec::new(),
            orphaned_worktrees: Vec::new(),
            stale_sessions: Vec::new(),
            total_cleaned: 0,
        }
    }

    pub fn add_branch(&mut self, branch_name: String) {
        self.orphaned_branches.push(branch_name);
        self.total_cleaned += 1;
    }

    pub fn add_worktree(&mut self, worktree_path: PathBuf) {
        self.orphaned_worktrees.push(worktree_path);
        self.total_cleaned += 1;
    }

    pub fn add_session(&mut self, session_id: String) {
        self.stale_sessions.push(session_id);
        self.total_cleaned += 1;
    }
}

impl Default for CleanupSummary {
    fn default() -> Self {
        Self::new()
    }
}
