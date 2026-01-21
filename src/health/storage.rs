//! Historical health metrics storage
//!
//! Stores health snapshots over time for trend analysis.

use std::path::PathBuf;
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::health::types::HealthOutput;

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
        let (cpu_sum, cpu_count) = output.shards.iter()
            .filter_map(|s| s.metrics.cpu_usage_percent)
            .fold((0.0, 0), |(sum, count), cpu| (sum + cpu, count + 1));

        let total_mem: u64 = output.shards.iter()
            .filter_map(|s| s.metrics.memory_usage_mb)
            .sum();

        Self {
            timestamp: Utc::now(),
            total_shards: output.total_count,
            working: output.working_count,
            idle: output.idle_count,
            stuck: output.stuck_count,
            crashed: output.crashed_count,
            avg_cpu_percent: if cpu_count > 0 { Some(cpu_sum / cpu_count as f32) } else { None },
            total_memory_mb: if total_mem > 0 { Some(total_mem) } else { None },
        }
    }
}

pub fn get_history_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".shards")
        .join("health_history")
}

pub fn save_snapshot(snapshot: &HealthSnapshot) -> Result<(), std::io::Error> {
    let history_dir = get_history_dir();
    fs::create_dir_all(&history_dir)?;

    let filename = format!("{}.json", snapshot.timestamp.format("%Y-%m-%d"));
    let filepath = history_dir.join(filename);

    // Append to daily file
    let mut snapshots: Vec<HealthSnapshot> = if filepath.exists() {
        let content = fs::read_to_string(&filepath)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    snapshots.push(snapshot.clone());
    fs::write(&filepath, serde_json::to_string_pretty(&snapshots)?)?;

    Ok(())
}

pub fn load_history(days: u64) -> Result<Vec<HealthSnapshot>, std::io::Error> {
    let history_dir = get_history_dir();
    let mut all_snapshots = Vec::new();

    let cutoff = Utc::now() - chrono::Duration::days(days as i64);

    if let Ok(entries) = fs::read_dir(&history_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(snapshots) = serde_json::from_str::<Vec<HealthSnapshot>>(&content) {
                    all_snapshots.extend(
                        snapshots.into_iter()
                            .filter(|s| s.timestamp > cutoff)
                    );
                }
            }
        }
    }

    all_snapshots.sort_by_key(|s| s.timestamp);
    Ok(all_snapshots)
}

pub fn cleanup_old_history(retention_days: u64) -> Result<usize, std::io::Error> {
    let history_dir = get_history_dir();
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

    let mut removed = 0;

    if let Ok(entries) = fs::read_dir(&history_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename < cutoff_date
                && filename.ends_with(".json")
                && fs::remove_file(entry.path()).is_ok()
            {
                removed += 1;
            }
        }
    }

    Ok(removed)
}
