use sysinfo::{Pid as SysinfoPid, ProcessesToUpdate, System};

use crate::process::errors::ProcessError;
use crate::process::types::{Pid, ProcessInfo, ProcessMetrics, ProcessStatus};

/// Check if a process with the given PID is currently running
pub fn is_process_running(pid: u32) -> Result<bool, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);
    Ok(system.process(pid_obj).is_some())
}

/// Kill a process with the given PID, validating it matches expected metadata
pub fn kill_process(
    pid: u32,
    expected_name: Option<&str>,
    expected_start_time: Option<u64>,
) -> Result<(), ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

    match system.process(pid_obj) {
        Some(process) => {
            // Validate process identity to prevent PID reuse attacks
            if let Some(name) = expected_name {
                let actual_name = process.name().to_string_lossy().to_string();
                if actual_name != name {
                    return Err(ProcessError::PidReused {
                        pid,
                        expected: name.to_string(),
                        actual: actual_name,
                    });
                }
            }

            if let Some(start_time) = expected_start_time
                && process.start_time() != start_time {
                    return Err(ProcessError::PidReused {
                        pid,
                        expected: format!("start_time={}", start_time),
                        actual: format!("start_time={}", process.start_time()),
                    });
                }

            if process.kill() {
                Ok(())
            } else {
                Err(ProcessError::KillFailed {
                    pid,
                    message: "Process kill signal failed".to_string(),
                })
            }
        }
        None => Err(ProcessError::NotFound { pid }),
    }
}

/// Get basic information about a process
pub fn get_process_info(pid: u32) -> Result<ProcessInfo, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

    match system.process(pid_obj) {
        Some(process) => Ok(ProcessInfo {
            pid: Pid::from_raw(pid),
            name: process.name().to_string_lossy().to_string(),
            status: ProcessStatus::from(process.status()),
            start_time: process.start_time(),
        }),
        None => Err(ProcessError::NotFound { pid }),
    }
}

/// Get CPU and memory usage metrics for a process
pub fn get_process_metrics(pid: u32) -> Result<ProcessMetrics, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    
    // Refresh process info with CPU usage
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);
    
    match system.process(pid_obj) {
        Some(process) => {
            let memory_bytes = process.memory();
            let memory_mb = memory_bytes / 1_024 / 1_024;
            
            Ok(ProcessMetrics {
                cpu_usage_percent: process.cpu_usage(),
                memory_usage_bytes: memory_bytes,
                memory_usage_mb: memory_mb,
            })
        }
        None => Err(ProcessError::NotFound { pid }),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn test_is_process_running_with_invalid_pid() {
        let result = is_process_running(999999);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_get_process_info_with_invalid_pid() {
        let result = get_process_info(999999);
        assert!(matches!(result, Err(ProcessError::NotFound { pid: 999999 })));
    }

    #[test]
    fn test_kill_process_with_invalid_pid() {
        let result = kill_process(999999, None, None);
        assert!(matches!(result, Err(ProcessError::NotFound { pid: 999999 })));
    }

    #[test]
    fn test_process_lifecycle() {
        let mut child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        let pid = child.id();

        let is_running = is_process_running(pid).expect("Failed to check process");
        assert!(is_running);

        let info = get_process_info(pid).expect("Failed to get process info");
        assert_eq!(info.pid.as_u32(), pid);
        assert!(info.name.contains("sleep"));

        let kill_result = kill_process(pid, Some(&info.name), Some(info.start_time));
        assert!(kill_result.is_ok());

        let _ = child.kill();
        let _ = child.wait();
    }
}
