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
    pub total_shards: usize,
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
            .shards
            .iter()
            .filter_map(|s| s.metrics.cpu_usage_percent)
            .fold((0.0, 0), |(sum, count), cpu| (sum + cpu, count + 1));

        let total_mem: u64 = output
            .shards
            .iter()
            .filter_map(|s| s.metrics.memory_usage_mb)
            .sum();

        Self {
            timestamp: Utc::now(),
            total_shards: output.total_count,
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
        .map(|p| p.join(".shards").join("health_history"))
}

pub fn save_snapshot(snapshot: &HealthSnapshot) -> Result<(), std::io::Error> {
    let history_dir = get_history_dir()?;
    fs::create_dir_all(&history_dir)?;

    let filename = format!("{}.json", snapshot.timestamp.format("%Y-%m-%d"));
    let filepath = history_dir.join(filename);

    // Append to daily file
    let mut snapshots: Vec<HealthSnapshot> = if filepath.exists() {
        let content = fs::read_to_string(&filepath)?;
        match serde_json::from_str(&content) {
            Ok(existing) => existing,
            Err(e) => {
                warn!(
                    event = "health.history_parse_failed",
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
    let mut all_snapshots = Vec::new();

    let cutoff = Utc::now() - chrono::Duration::days(days as i64);

    match fs::read_dir(&history_dir) {
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
                                            event = "health.history_file_parse_failed",
                                            file_path = %path.display(),
                                            error = %e,
                                            "Could not parse health history file - skipping"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(
                                    event = "health.history_file_read_failed",
                                    file_path = %path.display(),
                                    error = %e,
                                    "Could not read health history file - skipping"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            event = "health.history_dir_entry_failed",
                            error = %e,
                            "Could not read directory entry in health history"
                        );
                    }
                }
            }
        }
        Err(e) => {
            warn!(
                event = "health.history_dir_read_failed",
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
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

    let mut removed = 0;
    let mut failed = 0;

    match fs::read_dir(&history_dir) {
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
                                        event = "health.history_cleanup_delete_failed",
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
                            event = "health.history_cleanup_entry_failed",
                            error = %e,
                            "Could not read directory entry during cleanup"
                        );
                    }
                }
            }
        }
        Err(e) => {
            warn!(
                event = "health.history_cleanup_dir_read_failed",
                history_dir = %history_dir.display(),
                error = %e,
                "Could not read health history directory for cleanup"
            );
        }
    }

    if failed > 0 {
        warn!(
            event = "health.history_cleanup_partial",
            removed = removed,
            failed = failed,
            "Health history cleanup completed with some failures"
        );
    }

    Ok(CleanupResult { removed, failed })
}
