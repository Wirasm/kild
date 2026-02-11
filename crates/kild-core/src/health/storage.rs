//! Historical health metrics storage
//!
//! Stores health snapshots over time for trend analysis.

use crate::health::types::HealthOutput;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub timestamp: DateTime<Utc>,
    pub total_kilds: usize,
    pub working: usize,
    pub idle: usize,
    pub stuck: usize,
    pub crashed: usize,
    pub avg_cpu_percent: Option<f32>,
    pub total_memory_mb: Option<u64>,
}

impl From<&HealthOutput> for HealthSnapshot {
    fn from(output: &HealthOutput) -> Self {
        let (cpu_sum, cpu_count) = output
            .kilds
            .iter()
            .filter_map(|s| s.metrics.cpu_usage_percent)
            .fold((0.0, 0), |(sum, count), cpu| (sum + cpu, count + 1));

        let total_mem: u64 = output
            .kilds
            .iter()
            .filter_map(|s| s.metrics.memory_usage_mb)
            .sum();

        Self {
            timestamp: Utc::now(),
            total_kilds: output.total_count,
            working: output.working_count,
            idle: output.idle_count,
            stuck: output.stuck_count,
            crashed: output.crashed_count,
            avg_cpu_percent: if cpu_count > 0 {
                Some(cpu_sum / cpu_count as f32)
            } else {
                None
            },
            total_memory_mb: if total_mem > 0 { Some(total_mem) } else { None },
        }
    }
}

pub fn get_history_dir() -> Result<PathBuf, std::io::Error> {
    dirs::home_dir()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find home directory",
            )
        })
        .map(|p| p.join(".kild").join("health_history"))
}

pub fn save_snapshot(snapshot: &HealthSnapshot) -> Result<(), std::io::Error> {
    let history_dir = get_history_dir()?;
    save_snapshot_to(&history_dir, snapshot)
}

pub fn save_snapshot_to(
    history_dir: &std::path::Path,
    snapshot: &HealthSnapshot,
) -> Result<(), std::io::Error> {
    fs::create_dir_all(history_dir)?;

    let filename = format!("{}.json", snapshot.timestamp.format("%Y-%m-%d"));
    let filepath = history_dir.join(filename);

    // Append to daily file
    let mut snapshots: Vec<HealthSnapshot> = if filepath.exists() {
        let content = fs::read_to_string(&filepath)?;
        match serde_json::from_str(&content) {
            Ok(existing) => existing,
            Err(e) => {
                warn!(
                    event = "core.health.history_parse_failed",
                    file_path = %filepath.display(),
                    error = %e,
                    "Existing health history file is corrupted - starting fresh (previous data will be lost)"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    snapshots.push(snapshot.clone());
    fs::write(&filepath, serde_json::to_string_pretty(&snapshots)?)?;

    Ok(())
}

pub fn load_history(days: u64) -> Result<Vec<HealthSnapshot>, std::io::Error> {
    let history_dir = get_history_dir()?;
    load_history_from(&history_dir, days)
}

pub fn load_history_from(
    history_dir: &std::path::Path,
    days: u64,
) -> Result<Vec<HealthSnapshot>, std::io::Error> {
    let mut all_snapshots = Vec::new();

    let cutoff = Utc::now() - chrono::Duration::days(days as i64);

    match fs::read_dir(history_dir) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        match fs::read_to_string(&path) {
                            Ok(content) => {
                                match serde_json::from_str::<Vec<HealthSnapshot>>(&content) {
                                    Ok(snapshots) => {
                                        all_snapshots.extend(
                                            snapshots.into_iter().filter(|s| s.timestamp > cutoff),
                                        );
                                    }
                                    Err(e) => {
                                        warn!(
                                            event = "core.health.history_file_parse_failed",
                                            file_path = %path.display(),
                                            error = %e,
                                            "Could not parse health history file - skipping"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(
                                    event = "core.health.history_file_read_failed",
                                    file_path = %path.display(),
                                    error = %e,
                                    "Could not read health history file - skipping"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            event = "core.health.history_dir_entry_failed",
                            error = %e,
                            "Could not read directory entry in health history"
                        );
                    }
                }
            }
        }
        Err(e) => {
            warn!(
                event = "core.health.history_dir_read_failed",
                history_dir = %history_dir.display(),
                error = %e,
                "Could not read health history directory"
            );
        }
    }

    all_snapshots.sort_by_key(|s| s.timestamp);
    Ok(all_snapshots)
}

/// Result of history cleanup operation
#[derive(Debug)]
pub struct CleanupResult {
    pub removed: usize,
    pub failed: usize,
}

pub fn cleanup_old_history(retention_days: u64) -> Result<CleanupResult, std::io::Error> {
    let history_dir = get_history_dir()?;
    cleanup_old_history_in(&history_dir, retention_days)
}

pub fn cleanup_old_history_in(
    history_dir: &std::path::Path,
    retention_days: u64,
) -> Result<CleanupResult, std::io::Error> {
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

    let mut removed = 0;
    let mut failed = 0;

    match fs::read_dir(history_dir) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let filename = entry.file_name().to_string_lossy().to_string();
                        if filename < cutoff_date && filename.ends_with(".json") {
                            match fs::remove_file(entry.path()) {
                                Ok(()) => {
                                    removed += 1;
                                }
                                Err(e) => {
                                    failed += 1;
                                    warn!(
                                        event = "core.health.history_cleanup_delete_failed",
                                        file_path = %entry.path().display(),
                                        error = %e,
                                        "Could not delete old health history file"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            event = "core.health.history_cleanup_entry_failed",
                            error = %e,
                            "Could not read directory entry during cleanup"
                        );
                    }
                }
            }
        }
        Err(e) => {
            warn!(
                event = "core.health.history_cleanup_dir_read_failed",
                history_dir = %history_dir.display(),
                error = %e,
                "Could not read health history directory for cleanup"
            );
        }
    }

    if failed > 0 {
        warn!(
            event = "core.health.history_cleanup_partial",
            removed = removed,
            failed = failed,
            "Health history cleanup completed with some failures"
        );
    }

    Ok(CleanupResult { removed, failed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::types::*;
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    fn make_test_snapshot(working: usize, idle: usize, crashed: usize) -> HealthSnapshot {
        HealthSnapshot {
            timestamp: Utc::now(),
            total_kilds: working + idle + crashed,
            working,
            idle,
            stuck: 0,
            crashed,
            avg_cpu_percent: Some(15.0),
            total_memory_mb: Some(512),
        }
    }

    fn make_test_health_output() -> HealthOutput {
        let metrics = HealthMetrics {
            cpu_usage_percent: Some(20.0),
            memory_usage_mb: Some(256),
            process_status: "Running".to_string(),
            last_activity: None,
            status: HealthStatus::Working,
            status_icon: "check".to_string(),
        };
        let kild = KildHealth {
            session_id: "s1".to_string(),
            project_id: "p1".to_string(),
            branch: "main".to_string(),
            agent: "claude".to_string(),
            worktree_path: "/tmp/wt".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metrics,
        };
        HealthOutput {
            kilds: vec![kild],
            total_count: 1,
            working_count: 1,
            idle_count: 0,
            stuck_count: 0,
            crashed_count: 0,
        }
    }

    // --- HealthSnapshot From<&HealthOutput> tests ---

    #[test]
    fn test_snapshot_from_health_output_with_metrics() {
        let output = make_test_health_output();
        let snapshot = HealthSnapshot::from(&output);

        assert_eq!(snapshot.total_kilds, 1);
        assert_eq!(snapshot.working, 1);
        assert_eq!(snapshot.avg_cpu_percent, Some(20.0));
        assert_eq!(snapshot.total_memory_mb, Some(256));
    }

    #[test]
    fn test_snapshot_from_empty_output() {
        let output = HealthOutput {
            kilds: vec![],
            total_count: 0,
            working_count: 0,
            idle_count: 0,
            stuck_count: 0,
            crashed_count: 0,
        };
        let snapshot = HealthSnapshot::from(&output);

        assert_eq!(snapshot.total_kilds, 0);
        assert_eq!(snapshot.avg_cpu_percent, None);
        assert_eq!(snapshot.total_memory_mb, None);
    }

    #[test]
    fn test_snapshot_from_output_no_metrics() {
        let metrics = HealthMetrics {
            cpu_usage_percent: None,
            memory_usage_mb: None,
            process_status: "Stopped".to_string(),
            last_activity: None,
            status: HealthStatus::Crashed,
            status_icon: "x".to_string(),
        };
        let kild = KildHealth {
            session_id: "s1".to_string(),
            project_id: "p1".to_string(),
            branch: "b".to_string(),
            agent: "a".to_string(),
            worktree_path: "/tmp".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metrics,
        };
        let output = HealthOutput {
            kilds: vec![kild],
            total_count: 1,
            working_count: 0,
            idle_count: 0,
            stuck_count: 0,
            crashed_count: 1,
        };
        let snapshot = HealthSnapshot::from(&output);

        assert_eq!(snapshot.avg_cpu_percent, None);
        assert_eq!(snapshot.total_memory_mb, None);
    }

    // --- save_snapshot / load round-trip tests ---

    #[test]
    fn test_save_and_load_snapshot_roundtrip() {
        let dir = TempDir::new().unwrap();
        let snapshot = make_test_snapshot(2, 1, 0);

        save_snapshot_to(dir.path(), &snapshot).unwrap();

        let loaded = load_history_from(dir.path(), 1).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].working, 2);
        assert_eq!(loaded[0].idle, 1);
    }

    #[test]
    fn test_save_multiple_snapshots_same_day() {
        let dir = TempDir::new().unwrap();
        let s1 = make_test_snapshot(1, 0, 0);
        let s2 = make_test_snapshot(0, 1, 0);

        save_snapshot_to(dir.path(), &s1).unwrap();
        save_snapshot_to(dir.path(), &s2).unwrap();

        let loaded = load_history_from(dir.path(), 1).unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_load_history_empty_directory() {
        let dir = TempDir::new().unwrap();
        let loaded = load_history_from(dir.path(), 7).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_load_history_nonexistent_directory() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("does_not_exist");
        let loaded = load_history_from(&nonexistent, 7).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_load_history_skips_corrupted_files() {
        let dir = TempDir::new().unwrap();

        let snapshot = make_test_snapshot(1, 0, 0);
        save_snapshot_to(dir.path(), &snapshot).unwrap();

        let corrupted_path = dir.path().join("2025-01-01.json");
        fs::write(&corrupted_path, "not valid json").unwrap();

        let loaded = load_history_from(dir.path(), 365).unwrap();
        assert!(loaded.len() >= 1);
    }

    #[test]
    fn test_save_snapshot_overwrites_corrupted_daily_file() {
        let dir = TempDir::new().unwrap();

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let filepath = dir.path().join(format!("{}.json", today));
        fs::write(&filepath, "corrupted data").unwrap();

        let snapshot = make_test_snapshot(1, 0, 0);
        save_snapshot_to(dir.path(), &snapshot).unwrap();

        let loaded = load_history_from(dir.path(), 1).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    // --- cleanup tests ---

    #[test]
    fn test_cleanup_removes_old_files() {
        let dir = TempDir::new().unwrap();

        let old_date = (Utc::now() - Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        let old_file = dir.path().join(format!("{}.json", old_date));
        fs::write(&old_file, "[]").unwrap();

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let today_file = dir.path().join(format!("{}.json", today));
        fs::write(&today_file, "[]").unwrap();

        let result = cleanup_old_history_in(dir.path(), 7).unwrap();

        assert_eq!(result.removed, 1);
        assert_eq!(result.failed, 0);
        assert!(!old_file.exists());
        assert!(today_file.exists());
    }

    #[test]
    fn test_cleanup_empty_directory() {
        let dir = TempDir::new().unwrap();
        let result = cleanup_old_history_in(dir.path(), 7).unwrap();
        assert_eq!(result.removed, 0);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn test_cleanup_ignores_non_json_files() {
        let dir = TempDir::new().unwrap();

        let old_date = (Utc::now() - Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        let txt_file = dir.path().join(format!("{}.txt", old_date));
        fs::write(&txt_file, "not json").unwrap();

        let result = cleanup_old_history_in(dir.path(), 7).unwrap();
        assert_eq!(result.removed, 0);
        assert!(txt_file.exists());
    }

    #[test]
    fn test_cleanup_nonexistent_directory() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("nope");
        let result = cleanup_old_history_in(&nonexistent, 7).unwrap();
        assert_eq!(result.removed, 0);
        assert_eq!(result.failed, 0);
    }

    // --- load_history date filtering ---

    #[test]
    fn test_load_history_filters_by_days() {
        let dir = TempDir::new().unwrap();

        let old_ts = Utc::now() - Duration::days(10);
        let old_snapshot = HealthSnapshot {
            timestamp: old_ts,
            total_kilds: 1,
            working: 1,
            idle: 0,
            stuck: 0,
            crashed: 0,
            avg_cpu_percent: None,
            total_memory_mb: None,
        };
        let old_date = old_ts.format("%Y-%m-%d").to_string();
        let old_path = dir.path().join(format!("{}.json", old_date));
        fs::write(
            &old_path,
            serde_json::to_string(&vec![old_snapshot]).unwrap(),
        )
        .unwrap();

        let recent_snapshot = make_test_snapshot(2, 0, 0);
        save_snapshot_to(dir.path(), &recent_snapshot).unwrap();

        let loaded = load_history_from(dir.path(), 3).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].working, 2);
    }
}
