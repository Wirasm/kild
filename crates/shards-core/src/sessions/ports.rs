//! Port allocation and management
//!
//! Manages port range allocation for sessions to avoid conflicts.

use crate::sessions::{errors::SessionError, types::*};
use std::path::Path;

pub fn generate_session_id(project_id: &str, branch: &str) -> String {
    format!("{}/{}", project_id, branch)
}

pub fn calculate_port_range(session_index: u32) -> (u16, u16) {
    let base_port = 3000u16 + (session_index as u16 * 100);
    (base_port, base_port + 99)
}

pub fn allocate_port_range(
    sessions_dir: &Path,
    port_count: u16,
    base_port: u16,
) -> Result<(u16, u16), SessionError> {
    let (existing_sessions, _) = super::persistence::load_sessions_from_files(sessions_dir)?;

    // Find next available port range
    let (start_port, end_port) =
        find_next_available_range(&existing_sessions, port_count, base_port)?;

    Ok((start_port, end_port))
}

pub fn find_next_available_range(
    existing_sessions: &[Session],
    port_count: u16,
    base_port: u16,
) -> Result<(u16, u16), SessionError> {
    if port_count == 0 {
        return Err(SessionError::InvalidPortCount);
    }

    // Collect all allocated port ranges
    let mut allocated_ranges: Vec<(u16, u16)> = existing_sessions
        .iter()
        .map(|s| (s.port_range_start, s.port_range_end))
        .collect();

    // Sort by start port
    allocated_ranges.sort_by_key(|&(start, _)| start);

    // Try to find a gap starting from base_port
    let mut current_port = base_port;

    for &(allocated_start, allocated_end) in &allocated_ranges {
        let proposed_end = current_port
            .checked_add(port_count)
            .and_then(|sum| sum.checked_sub(1))
            .ok_or(SessionError::PortRangeExhausted)?;

        // Check if proposed range fits before this allocated range
        if proposed_end < allocated_start {
            return Ok((current_port, proposed_end));
        }

        // Move past this allocated range
        current_port = allocated_end + 1;
    }

    // Check if we can allocate after all existing ranges
    let proposed_end = current_port
        .checked_add(port_count)
        .and_then(|sum| sum.checked_sub(1))
        .ok_or(SessionError::PortRangeExhausted)?;

    Ok((current_port, proposed_end))
}

pub fn is_port_range_available(
    existing_sessions: &[Session],
    start_port: u16,
    end_port: u16,
) -> bool {
    for session in existing_sessions {
        // Check for overlap: ranges overlap if start1 <= end2 && start2 <= end1
        if start_port <= session.port_range_end && session.port_range_start <= end_port {
            return false;
        }
    }
    true
}

pub fn generate_port_env_vars(session: &Session) -> Vec<(String, String)> {
    vec![
        (
            "SHARD_PORT_RANGE_START".to_string(),
            session.port_range_start.to_string(),
        ),
        (
            "SHARD_PORT_RANGE_END".to_string(),
            session.port_range_end.to_string(),
        ),
        (
            "SHARD_PORT_COUNT".to_string(),
            session.port_count.to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_id() {
        let id = generate_session_id("my-project", "feature-branch");
        assert_eq!(id, "my-project/feature-branch");
    }

    #[test]
    fn test_calculate_port_range() {
        assert_eq!(calculate_port_range(0), (3000, 3099));
        assert_eq!(calculate_port_range(1), (3100, 3199));
        assert_eq!(calculate_port_range(5), (3500, 3599));
    }
}
