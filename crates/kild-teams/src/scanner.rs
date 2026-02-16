//! Scan for Claude Code team configs on disk.
//!
//! Enumerates `~/.claude/teams/*/config.json` to find active teams.

use std::path::{Path, PathBuf};

use crate::parser;
use crate::types::TeamState;

/// Default teams directory: `~/.claude/teams/`.
pub fn default_teams_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("teams"))
}

/// Scan a teams directory for all team configs.
///
/// Returns `(team_name, TeamState)` pairs for each successfully parsed config.
/// Silently skips directories with missing or malformed config files.
pub fn scan_teams(teams_dir: &Path) -> Vec<(String, TeamState)> {
    let entries = match std::fs::read_dir(teams_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(
                event = "teams.scanner.read_dir_failed",
                path = %teams_dir.display(),
                error = %e
            );
            return Vec::new();
        }
    };

    let mut teams = Vec::new();

    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let config_path = path.join("config.json");
        match parser::parse_team_config(&config_path) {
            Ok(Some(state)) => {
                tracing::debug!(
                    event = "teams.scanner.team_found",
                    team = state.team_name,
                    members = state.members.len()
                );
                teams.push((state.team_name.clone(), state));
            }
            Ok(None) => {
                // No config.json in this directory, skip
            }
            Err(e) => {
                tracing::warn!(
                    event = "teams.scanner.parse_failed",
                    path = %config_path.display(),
                    error = %e
                );
            }
        }
    }

    teams
}

/// Scan the default teams directory (`~/.claude/teams/`).
pub fn scan_teams_default() -> Vec<(String, TeamState)> {
    match default_teams_dir() {
        Some(dir) => scan_teams(&dir),
        None => {
            tracing::debug!(
                event = "teams.scanner.home_dir_unavailable",
                "Cannot determine home directory"
            );
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_teams_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let teams = scan_teams(dir.path());
        assert!(teams.is_empty());
    }

    #[test]
    fn test_scan_teams_with_teams() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create two team directories with config
        for name in &["team-alpha", "team-beta"] {
            let team_dir = dir.path().join(name);
            fs::create_dir_all(&team_dir).unwrap();
            fs::write(
                team_dir.join("config.json"),
                r#"{ "members": [{ "name": "worker", "tmuxPaneId": "%1" }] }"#,
            )
            .unwrap();
        }

        let teams = scan_teams(dir.path());
        assert_eq!(teams.len(), 2);

        let names: Vec<&str> = teams.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"team-alpha"));
        assert!(names.contains(&"team-beta"));
    }

    #[test]
    fn test_scan_teams_ignores_non_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        // Create a regular file (not a directory)
        fs::write(dir.path().join("not-a-team.txt"), "hello").unwrap();

        let teams = scan_teams(dir.path());
        assert!(teams.is_empty());
    }

    #[test]
    fn test_scan_teams_missing_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("nonexistent");

        let teams = scan_teams(&missing);
        assert!(teams.is_empty());
    }

    #[test]
    fn test_scan_teams_skips_malformed() {
        let dir = tempfile::TempDir::new().unwrap();

        // Valid team
        let good = dir.path().join("good-team");
        fs::create_dir_all(&good).unwrap();
        fs::write(
            good.join("config.json"),
            r#"{ "members": [{ "name": "a" }] }"#,
        )
        .unwrap();

        // Malformed team
        let bad = dir.path().join("bad-team");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("config.json"), "not json").unwrap();

        let teams = scan_teams(dir.path());
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].0, "good-team");
    }
}
