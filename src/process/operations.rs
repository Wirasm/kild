use sysinfo::{Pid as SysinfoPid, ProcessesToUpdate, System};

use crate::process::errors::ProcessError;
use crate::process::types::{Pid, ProcessInfo, ProcessStatus};

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

            if let Some(start_time) = expected_start_time {
                if process.start_time() != start_time {
                    return Err(ProcessError::PidReused {
                        pid,
                        expected: format!("start_time={}", start_time),
                        actual: format!("start_time={}", process.start_time()),
                    });
                }
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

    #[test]
    fn test_find_process_by_name() {
        use std::process::{Command, Stdio};

        // Spawn a test process
        let mut child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        // Give it a moment to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Should find it by name
        let result = find_process_by_name("sleep", None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Clean up
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_find_process_by_name_not_found() {
        let result = find_process_by_name("nonexistent-process-xyz", None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}

/// Find a process by name, optionally filtering by command line pattern
pub fn find_process_by_name(
    name_pattern: &str,
    command_pattern: Option<&str>,
) -> Result<Option<ProcessInfo>, ProcessError> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    for (pid, process) in system.processes() {
        let process_name = process.name().to_string_lossy();
        
        if !process_name.contains(name_pattern) {
            continue;
        }

        if let Some(cmd_pattern) = command_pattern {
            let cmd_line = process.cmd().iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            if !cmd_line.contains(cmd_pattern) {
                continue;
            }
        }

        return Ok(Some(ProcessInfo {
            pid: Pid::from_raw(pid.as_u32()),
            name: process_name.to_string(),
            status: ProcessStatus::from(process.status()),
            start_time: process.start_time(),
        }));
    }

    Ok(None)
}
