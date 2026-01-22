pub mod errors;
pub mod handler;
pub mod operations;
pub mod storage;
pub mod types;

// Re-export commonly used types
pub use errors::HealthError;
pub use handler::{get_health_all_sessions, get_health_single_session};
pub use operations::{get_idle_threshold_minutes, set_idle_threshold_minutes};
pub use storage::{HealthSnapshot, load_history, save_snapshot};
pub use types::{HealthMetrics, HealthOutput, HealthStatus, ShardHealth};
