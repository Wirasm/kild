use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    Working, // Process running, recent activity
    Idle,    // Process running, no activity >10min, last message from agent
    Stuck,   // Process running, no activity >10min, last message from user
    Crashed, // Process not running but session exists
    Unknown, // Cannot determine status
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub cpu_usage_percent: Option<f32>,
    pub memory_usage_mb: Option<u64>,
    pub process_status: String,
    pub last_activity: Option<String>,
    pub status: HealthStatus,
    pub status_icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardHealth {
    pub session_id: String,
    pub project_id: String,
    pub branch: String,
    pub agent: String,
    pub worktree_path: String,
    pub created_at: String,
    pub metrics: HealthMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthOutput {
    pub shards: Vec<ShardHealth>,
    pub total_count: usize,
    pub working_count: usize,
    pub idle_count: usize,
    pub stuck_count: usize,
    pub crashed_count: usize,
}
