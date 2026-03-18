//! Write fleet protocol instructions into agent-native instruction files in worktrees.
//!
//! Each agent backend has its own instruction file format. This module writes
//! a standardized fleet protocol section into the appropriate file, using
//! HTML comment markers for idempotent updates.

use std::path::Path;

use tracing::{debug, info, warn};

use crate::sessions::fleet;

const FLEET_BEGIN_MARKER: &str = "<!-- kild:fleet-protocol:begin -->";
const FLEET_END_MARKER: &str = "<!-- kild:fleet-protocol:end -->";

/// The universal fleet protocol instructions — identical for all agents.
fn fleet_protocol_text() -> &'static str {
    r#"## KILD Fleet Protocol

You are a worker in a KILD fleet managed by the Honryū brain supervisor.
Your inbox directory is at the path in the $KILD_INBOX environment variable.

After reading each new task:
1. Read $KILD_INBOX/task.md for your assignment
2. Write "working" to $KILD_INBOX/status
3. Execute the task fully
4. Write your results to $KILD_INBOX/report.md
5. Write "done" to $KILD_INBOX/status
6. Stop and wait for the next instruction

Do NOT modify task.md — it is written by the brain. Only write to status and report.md."#
}

/// Write fleet instructions into the appropriate agent-native instruction file.
///
/// No-op if fleet mode is not active, if the agent is a bare shell, or if
/// `is_main_worktree` is true (brain sessions run from the project root —
/// writing fleet instructions there would pollute the real CLAUDE.md).
pub(crate) fn setup_fleet_instructions(agent: &str, worktree_path: &Path, is_main_worktree: bool) {
    // Never write fleet instructions into the project root (--main sessions).
    // The brain has its own agent definition and doesn't need worktree instructions.
    if is_main_worktree {
        debug!(
            event = "core.fleet.instructions_skipped",
            agent = agent,
            reason = "main_worktree",
        );
        return;
    }

    if !fleet::fleet_mode_active(fleet::BRAIN_BRANCH) {
        debug!(
            event = "core.fleet.instructions_skipped",
            agent = agent,
            reason = "fleet_not_active",
        );
        return;
    }

    let result = match agent.to_lowercase().as_str() {
        "claude" => write_claude_fleet_instructions(worktree_path),
        "codex" | "amp" | "opencode" => write_agents_md_fleet_instructions(worktree_path),
        "gemini" => write_gemini_fleet_instructions(worktree_path),
        "kiro" => write_kiro_fleet_instructions(worktree_path),
        _ => {
            debug!(
                event = "core.fleet.instructions_skipped",
                agent = agent,
                reason = "unsupported_agent",
            );
            return;
        }
    };

    if let Err(e) = result {
        warn!(
            event = "core.fleet.instructions_write_failed",
            agent = agent,
            error = %e,
        );
        eprintln!(
            "Warning: Failed to write fleet instructions for '{}': {}",
            agent, e
        );
        eprintln!("The agent may not follow the fleet protocol automatically.");
    }
}

/// Write fleet instructions to `<worktree>/.claude/CLAUDE.md`.
fn write_claude_fleet_instructions(worktree_path: &Path) -> Result<(), String> {
    let claude_dir = worktree_path.join(".claude");
    let file_path = claude_dir.join("CLAUDE.md");

    std::fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("failed to create {}: {}", claude_dir.display(), e))?;

    let content = fleet_protocol_text();
    upsert_fleet_section(&file_path, content)?;

    info!(
        event = "core.fleet.instructions_written",
        agent = "claude",
        path = %file_path.display(),
    );
    Ok(())
}

/// Write fleet instructions to `<worktree>/AGENTS.md` (Codex, Amp, OpenCode).
fn write_agents_md_fleet_instructions(worktree_path: &Path) -> Result<(), String> {
    let file_path = worktree_path.join("AGENTS.md");
    let content = fleet_protocol_text();
    upsert_fleet_section(&file_path, content)?;

    info!(
        event = "core.fleet.instructions_written",
        agent = "agents_md",
        path = %file_path.display(),
    );
    Ok(())
}

/// Write fleet instructions to `<worktree>/GEMINI.md`.
fn write_gemini_fleet_instructions(worktree_path: &Path) -> Result<(), String> {
    let file_path = worktree_path.join("GEMINI.md");
    let content = fleet_protocol_text();
    upsert_fleet_section(&file_path, content)?;

    info!(
        event = "core.fleet.instructions_written",
        agent = "gemini",
        path = %file_path.display(),
    );
    Ok(())
}

/// Write fleet instructions to `<worktree>/.kiro/steering/kild-fleet.md`.
fn write_kiro_fleet_instructions(worktree_path: &Path) -> Result<(), String> {
    let steering_dir = worktree_path.join(".kiro").join("steering");
    let file_path = steering_dir.join("kild-fleet.md");

    std::fs::create_dir_all(&steering_dir)
        .map_err(|e| format!("failed to create {}: {}", steering_dir.display(), e))?;

    // Kiro steering files don't need markers — just write the full file.
    std::fs::write(&file_path, fleet_protocol_text())
        .map_err(|e| format!("failed to write {}: {}", file_path.display(), e))?;

    info!(
        event = "core.fleet.instructions_written",
        agent = "kiro",
        path = %file_path.display(),
    );
    Ok(())
}

/// Idempotent upsert of the fleet protocol section in a file.
///
/// If the file contains the begin marker, replaces the section between markers.
/// Otherwise, appends the section at the end. Creates the file if it doesn't exist.
fn upsert_fleet_section(file_path: &Path, content: &str) -> Result<(), String> {
    let section = format!(
        "\n{}\n{}\n{}\n",
        FLEET_BEGIN_MARKER, content, FLEET_END_MARKER
    );

    let existing = if file_path.exists() {
        std::fs::read_to_string(file_path)
            .map_err(|e| format!("failed to read {}: {}", file_path.display(), e))?
    } else {
        String::new()
    };

    let new_content = if let Some(begin_pos) = existing.find(FLEET_BEGIN_MARKER) {
        if let Some(end_pos) = existing.find(FLEET_END_MARKER) {
            // Replace existing section.
            let end = end_pos + FLEET_END_MARKER.len();
            // Include trailing newline if present.
            let end = if existing[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };
            format!(
                "{}{}{}",
                &existing[..begin_pos],
                section.trim_start_matches('\n'),
                &existing[end..]
            )
        } else {
            // Begin marker found but no end marker — append fresh section.
            format!("{}{}", existing, section)
        }
    } else {
        // No existing section — append.
        format!("{}{}", existing, section)
    };

    std::fs::write(file_path, new_content)
        .map_err(|e| format!("failed to write {}: {}", file_path.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kild_fleet_instructions_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_claude_instructions_creates_file() {
        let dir = temp_dir("claude_create");
        write_claude_fleet_instructions(&dir).unwrap();

        let path = dir.join(".claude/CLAUDE.md");
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("KILD Fleet Protocol"));
        assert!(content.contains(FLEET_BEGIN_MARKER));
        assert!(content.contains(FLEET_END_MARKER));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_claude_instructions_idempotent() {
        let dir = temp_dir("claude_idempotent");
        write_claude_fleet_instructions(&dir).unwrap();
        let content1 = fs::read_to_string(dir.join(".claude/CLAUDE.md")).unwrap();

        write_claude_fleet_instructions(&dir).unwrap();
        let content2 = fs::read_to_string(dir.join(".claude/CLAUDE.md")).unwrap();

        assert_eq!(content1, content2, "second call should not change content");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_agents_md_appends() {
        let dir = temp_dir("agents_append");
        fs::write(dir.join("AGENTS.md"), "# My Agents\n\nExisting content.\n").unwrap();

        write_agents_md_fleet_instructions(&dir).unwrap();

        let content = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(
            content.starts_with("# My Agents"),
            "existing content preserved"
        );
        assert!(
            content.contains("KILD Fleet Protocol"),
            "fleet section appended"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_fleet_section_replaces_existing() {
        let dir = temp_dir("upsert_replace");
        let file = dir.join("test.md");

        // Write initial content with markers
        let initial = format!(
            "# Header\n\n{}\nold content\n{}\n\n# Footer\n",
            FLEET_BEGIN_MARKER, FLEET_END_MARKER
        );
        fs::write(&file, &initial).unwrap();

        upsert_fleet_section(&file, "new content").unwrap();

        let result = fs::read_to_string(&file).unwrap();
        assert!(result.contains("new content"), "should have new content");
        assert!(
            !result.contains("old content"),
            "should not have old content"
        );
        assert!(result.contains("# Header"), "header preserved");
        assert!(result.contains("# Footer"), "footer preserved");
        // Should only have one pair of markers
        assert_eq!(
            result.matches(FLEET_BEGIN_MARKER).count(),
            1,
            "should have exactly one begin marker"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_kiro_fleet_instructions_creates_steering_file() {
        let dir = temp_dir("kiro_create");
        write_kiro_fleet_instructions(&dir).unwrap();

        let path = dir.join(".kiro/steering/kild-fleet.md");
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("KILD Fleet Protocol"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_gemini_fleet_instructions_creates_file() {
        let dir = temp_dir("gemini_create");
        write_gemini_fleet_instructions(&dir).unwrap();

        let path = dir.join("GEMINI.md");
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("KILD Fleet Protocol"));

        let _ = fs::remove_dir_all(&dir);
    }
}
