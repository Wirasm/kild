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

fn calculate_proposed_end(start_port: u16, port_count: u16) -> Result<u16, SessionError> {
    start_port
        .checked_add(port_count)
        .and_then(|sum| sum.checked_sub(1))
        .ok_or(SessionError::PortRangeExhausted)
}

pub fn find_next_available_range(
    existing_sessions: &[Session],
    port_count: u16,
    base_port: u16,
) -> Result<(u16, u16), SessionError> {
    if port_count == 0 {
        return Err(SessionError::InvalidPortCount);
    }

    // Collect and sort all allocated port ranges by start port
    let mut allocated_ranges: Vec<(u16, u16)> = existing_sessions
        .iter()
        .map(|s| (s.port_range_start, s.port_range_end))
        .collect();
    allocated_ranges.sort_by_key(|&(start, _)| start);

    let mut current_port = base_port;

    // Try to find a gap in the allocated ranges
    for &(allocated_start, allocated_end) in &allocated_ranges {
        let proposed_end = calculate_proposed_end(current_port, port_count)?;

        if proposed_end < allocated_start {
            return Ok((current_port, proposed_end));
        }

        current_port = allocated_end + 1;
    }

    // Allocate after all existing ranges
    let proposed_end = calculate_proposed_end(current_port, port_count)?;
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
            "KILD_PORT_RANGE_START".to_string(),
            session.port_range_start.to_string(),
        ),
        (
            "KILD_PORT_RANGE_END".to_string(),
            session.port_range_end.to_string(),
        ),
        (
            "KILD_PORT_COUNT".to_string(),
            session.port_count.to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_session_with_ports(start: u16, end: u16) -> Session {
        Session::new(
            format!("test/{}-{}", start, end),
            "test".to_string(),
            format!("branch-{}-{}", start, end),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            start,
            end,
            end - start + 1,
            None,
            None,
            vec![],
            None,
            None,
            None,
        )
    }

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

    #[test]
    fn test_find_next_available_range_empty_sessions() {
        let sessions: Vec<Session> = vec![];
        let result = find_next_available_range(&sessions, 10, 3000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (3000, 3009));
    }

    #[test]
    fn test_find_next_available_range_with_gap() {
        let sessions = vec![
            create_session_with_ports(3000, 3009),
            create_session_with_ports(3020, 3029), // Gap: 3010-3019
        ];
        let result = find_next_available_range(&sessions, 10, 3000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (3010, 3019)); // Should find the gap
    }

    #[test]
    fn test_find_next_available_range_no_gap_allocate_after() {
        let sessions = vec![
            create_session_with_ports(3000, 3009),
            create_session_with_ports(3010, 3019), // Contiguous, no gap
        ];
        let result = find_next_available_range(&sessions, 10, 3000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (3020, 3029)); // Allocate after existing ranges
    }

    #[test]
    fn test_find_next_available_range_zero_port_count() {
        let sessions: Vec<Session> = vec![];
        let result = find_next_available_range(&sessions, 0, 3000);
        assert!(matches!(result, Err(SessionError::InvalidPortCount)));
    }

    #[test]
    fn test_find_next_available_range_overflow_protection() {
        let sessions = vec![create_session_with_ports(65530, 65535)];
        let result = find_next_available_range(&sessions, 10, 65530);
        assert!(matches!(result, Err(SessionError::PortRangeExhausted)));
    }

    #[test]
    fn test_find_next_available_range_near_max_port() {
        let sessions: Vec<Session> = vec![];
        // Request a range that would overflow u16
        let result = find_next_available_range(&sessions, 10, 65530);
        assert!(matches!(result, Err(SessionError::PortRangeExhausted)));
    }

    #[test]
    fn test_is_port_range_available_no_overlap() {
        let sessions = vec![create_session_with_ports(3000, 3009)];
        // Range completely after existing
        assert!(is_port_range_available(&sessions, 3010, 3019));
        // Range completely before existing
        assert!(is_port_range_available(&sessions, 2990, 2999));
    }

    #[test]
    fn test_is_port_range_available_with_overlap() {
        let sessions = vec![create_session_with_ports(3000, 3009)];
        // Partial overlap at end
        assert!(!is_port_range_available(&sessions, 3005, 3015));
        // Partial overlap at start
        assert!(!is_port_range_available(&sessions, 2995, 3005));
        // Complete overlap (proposed contains existing)
        assert!(!is_port_range_available(&sessions, 2990, 3020));
        // Complete overlap (existing contains proposed)
        assert!(!is_port_range_available(&sessions, 3002, 3007));
    }

    #[test]
    fn test_is_port_range_available_edge_overlap() {
        let sessions = vec![create_session_with_ports(3000, 3009)];
        // Edge case: ranges touch at boundary (should overlap)
        assert!(!is_port_range_available(&sessions, 3009, 3015));
        assert!(!is_port_range_available(&sessions, 2995, 3000));
    }

    #[test]
    fn test_is_port_range_available_empty_sessions() {
        let sessions: Vec<Session> = vec![];
        assert!(is_port_range_available(&sessions, 3000, 3009));
    }

    #[test]
    fn test_generate_port_env_vars() {
        let session = create_session_with_ports(3000, 3009);
        let env_vars = generate_port_env_vars(&session);

        assert_eq!(env_vars.len(), 3);
        assert!(env_vars.contains(&("KILD_PORT_RANGE_START".to_string(), "3000".to_string())));
        assert!(env_vars.contains(&("KILD_PORT_RANGE_END".to_string(), "3009".to_string())));
        assert!(env_vars.contains(&("KILD_PORT_COUNT".to_string(), "10".to_string())));
    }

    #[test]
    fn test_generate_port_env_vars_names_are_correct() {
        let session = create_session_with_ports(8000, 8099);
        let env_vars = generate_port_env_vars(&session);

        // Verify exact env var names to catch typos
        let names: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"KILD_PORT_RANGE_START"));
        assert!(names.contains(&"KILD_PORT_RANGE_END"));
        assert!(names.contains(&"KILD_PORT_COUNT"));
    }

    #[test]
    fn test_allocate_port_range_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (start, end) = allocate_port_range(tmp.path(), 10, 3000).unwrap();
        assert_eq!(start, 3000);
        assert_eq!(end, 3009);
    }

    #[test]
    fn test_allocate_port_range_avoids_existing_session() {
        let tmp = tempfile::TempDir::new().unwrap();

        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();

        let mut existing = Session::new_for_test("existing".to_string(), wt);
        existing.port_range_start = 3000;
        existing.port_range_end = 3009;
        existing.port_count = 10;
        super::super::persistence::save_session_to_file(&existing, tmp.path()).unwrap();

        let (start, end) = allocate_port_range(tmp.path(), 10, 3000).unwrap();
        assert_eq!(start, 3010);
        assert_eq!(end, 3019);
    }

    #[test]
    fn test_allocate_port_range_nonexistent_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does_not_exist");

        let (start, end) = allocate_port_range(&nonexistent, 10, 3000).unwrap();
        assert_eq!(start, 3000);
        assert_eq!(end, 3009);
    }

    #[test]
    fn test_allocate_port_range_custom_port_count() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (start, end) = allocate_port_range(tmp.path(), 5, 8000).unwrap();
        assert_eq!(start, 8000);
        assert_eq!(end, 8004);
    }
}
