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
