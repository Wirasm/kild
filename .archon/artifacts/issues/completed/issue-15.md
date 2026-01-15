# Investigation: Cleanup command doesn't detect stale sessions with no PID

**Issue**: #15 (https://github.com/Wirasm/shards/issues/15)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-15T14:42:28+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | HIGH | Affects maintenance workflows and leaves stale data accumulating; user reported 14+ stale sessions with no cleanup path |
| Complexity | MEDIUM | Requires changes to 3 files (operations.rs, handler.rs, app.rs) with new detection logic and CLI flags, but follows existing patterns |
| Confidence | HIGH | Clear root cause identified in operations.rs:detect_stale_sessions() which only checks worktree existence, not PID status; existing process validation utilities available |

---

## Problem Statement

The `shards cleanup` command only detects orphaned resources (missing worktrees/branches) but doesn't clean up stale sessions that have no PID tracking or stopped processes. Users accumulate sessions with "No PID" status that cannot be cleaned up automatically.

---

## Analysis

### Root Cause / Change Rationale

The cleanup system was designed to detect structural orphans (missing Git resources) but doesn't consider process lifecycle state. Sessions can become stale in multiple ways beyond missing worktrees:

1. **Sessions with no PID tracking** - Created before PID tracking was implemented (`process_id: None`)
2. **Sessions with stopped processes** - PID exists but process is no longer running
3. **Old sessions** - Sessions created long ago that may be abandoned

### Evidence Chain

WHY: Cleanup doesn't remove sessions with "No PID" status
↓ BECAUSE: `detect_stale_sessions()` only checks if worktree path exists
  Evidence: `src/cleanup/operations.rs:119-145` - Only validates worktree_path existence

```rust
// Line 119-145
pub fn detect_stale_sessions(sessions_dir: &Path) -> Result<Vec<String>, CleanupError> {
    // ... reads session files ...
    if let Some(worktree_path) = session.get("worktree_path").and_then(|v| v.as_str()) {
        let worktree_path = PathBuf::from(worktree_path);
        if !worktree_path.exists() {
            // Session references non-existent worktree
            if let Some(session_id) = session.get("id").and_then(|v| v.as_str()) {
                stale_sessions.push(session_id.to_string());
            }
        }
    }
}
```

↓ BECAUSE: No logic checks `process_id` field or process running status
  Evidence: `src/sessions/types.rs:33-39` - Session has process_id field but cleanup doesn't use it

```rust
// Line 33-39
pub process_id: Option<u32>,
pub process_name: Option<String>,
pub process_start_time: Option<u64>,
```

↓ ROOT CAUSE: Missing detection strategies for PID-based staleness
  Evidence: `src/cleanup/operations.rs:119` - Function signature doesn't support filtering strategies

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/cleanup/operations.rs` | 119-145 | UPDATE | Add new detection functions for no-PID and stopped-process sessions |
| `src/cleanup/handler.rs` | 1-300 | UPDATE | Add conditional cleanup based on strategy flags |
| `src/cli/app.rs` | 68-71 | UPDATE | Add cleanup command flags (--no-pid, --stopped, --older-than, --interactive) |
| `src/cleanup/types.rs` | NEW | UPDATE | Add CleanupStrategy enum |

### Integration Points

- `src/process/operations.rs:9` - `is_process_running()` can validate PIDs
- `src/sessions/operations.rs` - Session loading/removal functions
- `src/cli/commands.rs:251` - `handle_cleanup_command()` calls cleanup::cleanup_all()

### Git History

- **Introduced**: c419e39 - 2026-01-13 - "feat: implement cleanup tracking system (#3)"
- **Last modified**: c419e39 - 2026-01-13
- **Implication**: Recent feature, designed for structural orphans only, not process lifecycle

---

## Implementation Plan

### Step 1: Add cleanup strategy types

**File**: `src/cleanup/types.rs`
**Lines**: 1-68
**Action**: UPDATE

**Current code:**
```rust
// Line 1-68 - Only has ResourceType and CleanupSummary
```

**Required change:**
```rust
// Add after ResourceType enum
#[derive(Debug, Clone, PartialEq)]
pub enum CleanupStrategy {
    All,           // Clean everything (default)
    NoPid,         // Only sessions with process_id: None
    Stopped,       // Only sessions with stopped processes
    OlderThan(u64), // Only sessions older than N days
}
```

**Why**: Need to represent different cleanup strategies for conditional detection

---

### Step 2: Add PID-based detection functions

**File**: `src/cleanup/operations.rs`
**Lines**: 145 (after detect_stale_sessions)
**Action**: UPDATE

**Required change:**
```rust
// Add new detection functions after detect_stale_sessions()

pub fn detect_sessions_without_pid(sessions_dir: &Path) -> Result<Vec<String>, CleanupError> {
    let mut sessions_without_pid = Vec::new();
    
    if !sessions_dir.exists() {
        return Ok(sessions_without_pid);
    }
    
    let entries = std::fs::read_dir(sessions_dir).map_err(|e| CleanupError::IoError { source: e })?;
    
    for entry in entries {
        let entry = entry.map_err(|e| CleanupError::IoError { source: e })?;
        let path = entry.path();
        
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(session) = serde_json::from_str::<serde_json::Value>(&content) {
                        // Check if process_id is null or missing
                        if session.get("process_id").and_then(|v| v.as_u64()).is_none() {
                            if let Some(session_id) = session.get("id").and_then(|v| v.as_str()) {
                                sessions_without_pid.push(session_id.to_string());
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    }
    
    Ok(sessions_without_pid)
}

pub fn detect_sessions_with_stopped_processes(sessions_dir: &Path) -> Result<Vec<String>, CleanupError> {
    let mut stopped_sessions = Vec::new();
    
    if !sessions_dir.exists() {
        return Ok(stopped_sessions);
    }
    
    let entries = std::fs::read_dir(sessions_dir).map_err(|e| CleanupError::IoError { source: e })?;
    
    for entry in entries {
        let entry = entry.map_err(|e| CleanupError::IoError { source: e })?;
        let path = entry.path();
        
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(session) = serde_json::from_str::<serde_json::Value>(&content) {
                        // Check if process_id exists and process is not running
                        if let Some(pid) = session.get("process_id").and_then(|v| v.as_u64()) {
                            match crate::process::is_process_running(pid as u32) {
                                Ok(false) => {
                                    if let Some(session_id) = session.get("id").and_then(|v| v.as_str()) {
                                        stopped_sessions.push(session_id.to_string());
                                    }
                                }
                                Ok(true) => continue,
                                Err(_) => continue, // Process check failed, skip
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    }
    
    Ok(stopped_sessions)
}

pub fn detect_old_sessions(sessions_dir: &Path, days: u64) -> Result<Vec<String>, CleanupError> {
    let mut old_sessions = Vec::new();
    
    if !sessions_dir.exists() {
        return Ok(old_sessions);
    }
    
    let cutoff_timestamp = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let entries = std::fs::read_dir(sessions_dir).map_err(|e| CleanupError::IoError { source: e })?;
    
    for entry in entries {
        let entry = entry.map_err(|e| CleanupError::IoError { source: e })?;
        let path = entry.path();
        
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(session) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(created_at) = session.get("created_at").and_then(|v| v.as_str()) {
                            if let Ok(created_time) = chrono::DateTime::parse_from_rfc3339(created_at) {
                                if created_time.with_timezone(&chrono::Utc) < cutoff_timestamp {
                                    if let Some(session_id) = session.get("id").and_then(|v| v.as_str()) {
                                        old_sessions.push(session_id.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    }
    
    Ok(old_sessions)
}
```

**Why**: Implement the three new detection strategies for PID-based staleness

---

### Step 3: Update handler to support strategies

**File**: `src/cleanup/handler.rs`
**Lines**: 1-10
**Action**: UPDATE

**Current code:**
```rust
// Line 1-10
use tracing::{error, info, warn};
use git2::{BranchType, Repository};

use crate::cleanup::{errors::CleanupError, operations, types::*};
use crate::core::config::Config;
use crate::git;
use crate::sessions;

pub fn scan_for_orphans() -> Result<CleanupSummary, CleanupError> {
```

**Required change:**
```rust
use tracing::{error, info, warn};
use git2::{BranchType, Repository};

use crate::cleanup::{errors::CleanupError, operations, types::*};
use crate::core::config::Config;
use crate::git;
use crate::sessions;

pub fn scan_for_orphans_with_strategy(strategy: CleanupStrategy) -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.scan_started", strategy = ?strategy);

    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let repo = Repository::discover(&current_dir).map_err(|_| CleanupError::NotInRepository)?;

    let mut summary = CleanupSummary::new();
    let config = Config::new();

    match strategy {
        CleanupStrategy::All => {
            // Detect all types of orphans (existing behavior)
            match operations::detect_orphaned_branches(&repo) {
                Ok(branches) => {
                    for branch in branches {
                        summary.add_branch(branch);
                    }
                }
                Err(e) => return Err(e),
            }

            match operations::detect_orphaned_worktrees(&repo) {
                Ok(worktrees) => {
                    for worktree_path in worktrees {
                        summary.add_worktree(worktree_path);
                    }
                }
                Err(e) => return Err(e),
            }

            match operations::detect_stale_sessions(&config.sessions_dir()) {
                Ok(sessions) => {
                    for session_id in sessions {
                        summary.add_session(session_id);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        CleanupStrategy::NoPid => {
            // Only detect sessions without PID
            match operations::detect_sessions_without_pid(&config.sessions_dir()) {
                Ok(sessions) => {
                    for session_id in sessions {
                        summary.add_session(session_id);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        CleanupStrategy::Stopped => {
            // Only detect sessions with stopped processes
            match operations::detect_sessions_with_stopped_processes(&config.sessions_dir()) {
                Ok(sessions) => {
                    for session_id in sessions {
                        summary.add_session(session_id);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        CleanupStrategy::OlderThan(days) => {
            // Only detect old sessions
            match operations::detect_old_sessions(&config.sessions_dir(), days) {
                Ok(sessions) => {
                    for session_id in sessions {
                        summary.add_session(session_id);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    info!(
        event = "cleanup.scan_completed",
        total_orphaned = summary.total_cleaned
    );

    Ok(summary)
}

// Keep existing scan_for_orphans for backward compatibility
pub fn scan_for_orphans() -> Result<CleanupSummary, CleanupError> {
    scan_for_orphans_with_strategy(CleanupStrategy::All)
}
```

**Why**: Add strategy-based scanning while maintaining backward compatibility

---

### Step 4: Update cleanup_all to accept strategy

**File**: `src/cleanup/handler.rs`
**Lines**: 150-165
**Action**: UPDATE

**Current code:**
```rust
pub fn cleanup_all() -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.cleanup_all_started");

    // First scan for orphaned resources
    let scan_summary = scan_for_orphans()?;

    if scan_summary.total_cleaned == 0 {
        info!(event = "cleanup.cleanup_all_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    // Then clean them up
    let cleanup_summary = cleanup_orphaned_resources(&scan_summary)?;

    info!(
        event = "cleanup.cleanup_all_completed",
        total_cleaned = cleanup_summary.total_cleaned
    );

    Ok(cleanup_summary)
}
```

**Required change:**
```rust
pub fn cleanup_all_with_strategy(strategy: CleanupStrategy) -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.cleanup_all_started", strategy = ?strategy);

    let scan_summary = scan_for_orphans_with_strategy(strategy)?;

    if scan_summary.total_cleaned == 0 {
        info!(event = "cleanup.cleanup_all_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    let cleanup_summary = cleanup_orphaned_resources(&scan_summary)?;

    info!(
        event = "cleanup.cleanup_all_completed",
        total_cleaned = cleanup_summary.total_cleaned
    );

    Ok(cleanup_summary)
}

// Keep existing cleanup_all for backward compatibility
pub fn cleanup_all() -> Result<CleanupSummary, CleanupError> {
    cleanup_all_with_strategy(CleanupStrategy::All)
}
```

**Why**: Support strategy parameter while maintaining existing API

---

### Step 5: Add CLI flags

**File**: `src/cli/app.rs`
**Lines**: 68-71
**Action**: UPDATE

**Current code:**
```rust
.subcommand(
    Command::new("cleanup")
        .about("Clean up orphaned resources (branches, worktrees, sessions)")
)
```

**Required change:**
```rust
.subcommand(
    Command::new("cleanup")
        .about("Clean up orphaned resources (branches, worktrees, sessions)")
        .arg(
            Arg::new("no-pid")
                .long("no-pid")
                .help("Clean only sessions without PID tracking")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("stopped")
                .long("stopped")
                .help("Clean only sessions with stopped processes")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("older-than")
                .long("older-than")
                .help("Clean sessions older than N days (e.g., 7)")
                .value_name("DAYS")
                .value_parser(clap::value_parser!(u64))
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Clean all orphaned resources (default)")
                .action(ArgAction::SetTrue)
        )
)
```

**Why**: Add command-line flags for different cleanup strategies

---

### Step 6: Update command handler

**File**: `src/cli/commands.rs`
**Lines**: 251-280
**Action**: UPDATE

**Current code:**
```rust
fn handle_cleanup_command() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");

    match cleanup::cleanup_all() {
        Ok(summary) => {
            // ... display results ...
        }
        Err(e) => {
            // ... handle errors ...
        }
    }
}
```

**Required change:**
```rust
fn handle_cleanup_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");

    // Determine cleanup strategy from flags
    let strategy = if matches.get_flag("no-pid") {
        cleanup::types::CleanupStrategy::NoPid
    } else if matches.get_flag("stopped") {
        cleanup::types::CleanupStrategy::Stopped
    } else if let Some(days) = matches.get_one::<u64>("older-than") {
        cleanup::types::CleanupStrategy::OlderThan(*days)
    } else {
        cleanup::types::CleanupStrategy::All
    };

    match cleanup::cleanup_all_with_strategy(strategy) {
        Ok(summary) => {
            println!("\n✅ Cleanup completed successfully!\n");

            if !summary.orphaned_branches.is_empty() {
                println!("🌿 Cleaned branches:");
                for branch in &summary.orphaned_branches {
                    println!("   - {}", branch);
                }
                println!();
            }

            if !summary.orphaned_worktrees.is_empty() {
                println!("📁 Cleaned worktrees:");
                for worktree in &summary.orphaned_worktrees {
                    println!("   - {}", worktree.display());
                }
                println!();
            }

            if !summary.stale_sessions.is_empty() {
                println!("🗑️  Cleaned sessions:");
                for session in &summary.stale_sessions {
                    println!("   - {}", session);
                }
                println!();
            }

            println!("Total resources cleaned: {}", summary.total_cleaned);
            Ok(())
        }
        Err(cleanup::errors::CleanupError::NoOrphanedResources) => {
            println!("✨ No orphaned resources found. Everything is clean!");
            Ok(())
        }
        Err(e) => {
            error!(event = "cli.cleanup_failed", error = %e);
            eprintln!("❌ Cleanup failed: {}", e);
            Err(Box::new(e))
        }
    }
}
```

**Why**: Parse CLI flags and pass strategy to cleanup handler

---

### Step 7: Update command dispatcher

**File**: `src/cli/commands.rs`
**Lines**: 18
**Action**: UPDATE

**Current code:**
```rust
Some(("cleanup", _)) => handle_cleanup_command(),
```

**Required change:**
```rust
Some(("cleanup", sub_matches)) => handle_cleanup_command(sub_matches),
```

**Why**: Pass subcommand matches to handler for flag parsing

---

### Step 8: Add tests for new detection functions

**File**: `src/cleanup/operations.rs`
**Lines**: 340 (after existing tests)
**Action**: UPDATE

**Required change:**
```rust
#[test]
fn test_detect_sessions_without_pid() {
    let temp_dir = env::temp_dir().join("shards_test_no_pid");
    let _ = fs::create_dir_all(&temp_dir);

    // Session with PID
    let with_pid = serde_json::json!({
        "id": "with-pid",
        "process_id": 12345,
        "worktree_path": temp_dir.to_str().unwrap(),
    });
    fs::write(&temp_dir.join("with-pid.json"), with_pid.to_string()).unwrap();

    // Session without PID
    let without_pid = serde_json::json!({
        "id": "without-pid",
        "process_id": null,
        "worktree_path": temp_dir.to_str().unwrap(),
    });
    fs::write(&temp_dir.join("without-pid.json"), without_pid.to_string()).unwrap();

    let result = detect_sessions_without_pid(&temp_dir);
    assert!(result.is_ok());
    let sessions = result.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0], "without-pid");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_old_sessions() {
    let temp_dir = env::temp_dir().join("shards_test_old");
    let _ = fs::create_dir_all(&temp_dir);

    // Recent session
    let recent = serde_json::json!({
        "id": "recent",
        "created_at": chrono::Utc::now().to_rfc3339(),
        "worktree_path": temp_dir.to_str().unwrap(),
    });
    fs::write(&temp_dir.join("recent.json"), recent.to_string()).unwrap();

    // Old session (10 days ago)
    let old_time = chrono::Utc::now() - chrono::Duration::days(10);
    let old = serde_json::json!({
        "id": "old",
        "created_at": old_time.to_rfc3339(),
        "worktree_path": temp_dir.to_str().unwrap(),
    });
    fs::write(&temp_dir.join("old.json"), old.to_string()).unwrap();

    let result = detect_old_sessions(&temp_dir, 7);
    assert!(result.is_ok());
    let sessions = result.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0], "old");

    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Why**: Ensure new detection functions work correctly

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/cleanup/operations.rs:119-145
// Pattern for session file iteration and JSON parsing
pub fn detect_stale_sessions(sessions_dir: &Path) -> Result<Vec<String>, CleanupError> {
    let mut stale_sessions = Vec::new();
    
    if !sessions_dir.exists() {
        return Ok(stale_sessions);
    }
    
    let entries = std::fs::read_dir(sessions_dir).map_err(|e| CleanupError::IoError { source: e })?;
    
    for entry in entries {
        let entry = entry.map_err(|e| CleanupError::IoError { source: e })?;
        let path = entry.path();
        
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        // ... validation logic ...
                    }
                }
                Err(_) => continue,
            }
        }
    }
    
    Ok(stale_sessions)
}
```

```rust
// SOURCE: src/process/operations.rs:9-11
// Pattern for checking if process is running
pub fn is_process_running(pid: u32) -> Result<bool, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);
    Ok(system.process(pid_obj).is_some())
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| PID reuse - process with same PID but different program | Use process_name and process_start_time validation (already in codebase) |
| Concurrent cleanup operations | Existing race condition handling in cleanup_orphaned_branches handles this |
| Invalid JSON in session files | Existing error handling with `continue` on parse failure |
| Sessions with PID but process check fails | Skip with `continue` to avoid false positives |
| Multiple flags specified | First flag wins (no-pid > stopped > older-than > all) |
| Empty sessions directory | Early return with empty vec (existing pattern) |

---

## Validation

### Automated Checks

```bash
cargo test cleanup::operations::test_detect_sessions_without_pid
cargo test cleanup::operations::test_detect_old_sessions
cargo test cleanup::handler
cargo fmt
cargo clippy
```

### Manual Verification

1. Create test sessions with no PID: `shards create test-no-pid --agent claude` (then manually edit JSON to remove process_id)
2. Run `shards cleanup --no-pid` and verify it detects the session
3. Run `shards cleanup --stopped` and verify it detects stopped processes
4. Run `shards cleanup --older-than 7` and verify it detects old sessions
5. Run `shards cleanup --all` and verify it detects all types (existing behavior)
6. Verify `shards list` shows cleaned sessions are gone

---

## Scope Boundaries

**IN SCOPE:**
- Add detection for sessions without PID
- Add detection for sessions with stopped processes
- Add detection for old sessions based on age
- Add CLI flags for cleanup strategies
- Update handler to support strategies
- Add tests for new detection functions

**OUT OF SCOPE (do not touch):**
- Interactive mode (--interactive flag) - defer to future PR
- Changing existing orphan detection logic
- Modifying session structure or PID tracking
- Process monitoring or health checks
- GUI or TUI interface

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-15T14:42:28+02:00
- **Artifact**: `.archon/artifacts/issues/issue-15.md`
