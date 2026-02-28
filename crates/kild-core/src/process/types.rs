use serde::{Deserialize, Serialize};
use sysinfo::Pid as SysinfoPid;

/// Platform-safe process ID wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pid(u32);

impl Pid {
    pub fn new(pid: u32) -> Result<Self, crate::process::errors::ProcessError> {
        if pid == 0 {
            return Err(crate::process::errors::ProcessError::InvalidPid { pid });
        }
        Ok(Self(pid))
    }

    pub fn from_raw(pid: u32) -> Self {
        Self(pid)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn to_sysinfo_pid(&self) -> SysinfoPid {
        SysinfoPid::from_u32(self.0)
    }
}

impl From<u32> for Pid {
    fn from(pid: u32) -> Self {
        Self(pid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    Running,
    Sleeping,
    Stopped,
    Zombie,
    Dead,
    Unknown(String),
}

impl From<sysinfo::ProcessStatus> for ProcessStatus {
    fn from(status: sysinfo::ProcessStatus) -> Self {
        let status_str = status.to_string();
        match status_str.as_str() {
            "Run" | "Running" => ProcessStatus::Running,
            "Sleep" | "Sleeping" => ProcessStatus::Sleeping,
            "Stop" | "Stopped" => ProcessStatus::Stopped,
            "Zombie" => ProcessStatus::Zombie,
            "Dead" => ProcessStatus::Dead,
            _ => ProcessStatus::Unknown(status_str),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: Pid,
    pub name: String,
    pub status: ProcessStatus,
    pub start_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetadata {
    pub name: String,
    pub start_time: u64,
}

impl From<&ProcessInfo> for ProcessMetadata {
    fn from(info: &ProcessInfo) -> Self {
        Self {
            name: info.name.clone(),
            start_time: info.start_time,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetrics {
    pub cpu_usage_percent: f32,
    pub memory_usage_bytes: u64,
}

impl ProcessMetrics {
    pub fn memory_usage_mb(&self) -> u64 {
        self.memory_usage_bytes / 1_024 / 1_024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_metadata_from_process_info() {
        let info = ProcessInfo {
            pid: Pid::from_raw(1234),
            name: "claude".to_string(),
            status: ProcessStatus::Running,
            start_time: 1700000000,
        };
        let metadata: ProcessMetadata = ProcessMetadata::from(&info);
        assert_eq!(metadata.name, "claude");
        assert_eq!(metadata.start_time, 1700000000);
    }

    #[test]
    fn test_process_metadata_from_ref_preserves_original() {
        let info = ProcessInfo {
            pid: Pid::from_raw(5678),
            name: "kiro".to_string(),
            status: ProcessStatus::Sleeping,
            start_time: 1700001000,
        };
        let _metadata: ProcessMetadata = (&info).into();
        assert_eq!(info.name, "kiro");
        assert_eq!(info.pid.as_u32(), 5678);
    }

    #[test]
    fn test_pid_from_u32() {
        let pid: Pid = 42u32.into();
        assert_eq!(pid.as_u32(), 42);
    }

    #[test]
    fn test_pid_new_rejects_zero() {
        assert!(Pid::new(0).is_err());
    }

    #[test]
    fn test_pid_new_accepts_nonzero() {
        assert!(Pid::new(1).is_ok());
    }

    #[test]
    fn test_process_metrics_memory_mb() {
        let metrics = ProcessMetrics {
            cpu_usage_percent: 10.0,
            memory_usage_bytes: 1_024 * 1_024 * 256,
        };
        assert_eq!(metrics.memory_usage_mb(), 256);
    }
}
