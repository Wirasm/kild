# Investigation: Wire cleanup strategy functions to CLI flags

**Issue**: #27 (https://github.com/Wirasm/shards/issues/27)
**Type**: BUG
**Investigated**: 2026-01-20T15:13:43.605+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | MEDIUM | Feature partially broken - cleanup strategies exist but aren't accessible to users, workaround exists (use default cleanup) |
| Complexity | LOW | 1-2 files affected, isolated change to wire existing functions to CLI flags, low risk |
| Confidence | HIGH | Clear root cause identified - CLI flags exist but aren't parsed, detection functions exist but unused |

---

## Problem Statement

The cleanup module has implemented detection functions for different cleanup strategies, but they are not connected to the CLI flags, causing build warnings for unused code and making the functionality inaccessible to users.

---

## Analysis

### Root Cause / Change Rationale

The CLI flags (`--no-pid`, `--stopped`, `--older-than`) are defined in the clap app but not parsed or used in the command handler. The detection functions exist in operations.rs but are never called.

### Evidence Chain

WHY: Build warnings for unused functions
↓ BECAUSE: Detection functions are never called
  Evidence: `src/cleanup/operations.rs:167` - `pub fn detect_sessions_without_pid()` exists but unused

↓ BECAUSE: CLI command handler doesn't parse strategy flags
  Evidence: `src/cli/commands.rs:298` - `handle_cleanup_command()` only calls `cleanup::cleanup_all()`

↓ ROOT CAUSE: Missing strategy parsing and missing strategy-specific handler functions
  Evidence: `src/cli/commands.rs:298-301` - No flag parsing, direct call to cleanup_all()

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/cli/commands.rs` | 298-350 | UPDATE | Parse cleanup strategy flags and call appropriate functions |
| `src/cleanup/handler.rs` | NEW | UPDATE | Add missing strategy functions (cleanup_all_with_strategy, scan_for_orphans_with_strategy) |
| `src/cleanup/mod.rs` | 6-9 | UPDATE | Export new strategy functions |

### Integration Points

- `src/cli/commands.rs:32` calls `handle_cleanup_command()`
- `src/cleanup/operations.rs:167,201,245` contains unused detection functions
- `src/cleanup/types.rs:11` defines `CleanupStrategy` enum with variants

### Git History

- **Introduced**: 15841ab - 2026-01-20 - "feat: add cleanup strategies for stale sessions (#15) (#23)"
- **Last modified**: 15841ab - same commit
- **Implication**: Recent feature addition where CLI wiring was missed

---

## Implementation Plan

### Step 1: Parse cleanup strategy flags in CLI handler

**File**: `src/cli/commands.rs`
**Lines**: 298-301
**Action**: UPDATE

**Current code:**
```rust
fn handle_cleanup_command() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");

    match cleanup::cleanup_all() {
```

**Required change:**
```rust
fn handle_cleanup_command() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");
    
    let matches = crate::cli::app::build_app().get_matches();
    let cleanup_matches = matches.subcommand_matches("cleanup").unwrap();
    
    let strategy = if cleanup_matches.get_flag("no-pid") {
        cleanup::CleanupStrategy::NoPid
    } else if cleanup_matches.get_flag("stopped") {
        cleanup::CleanupStrategy::Stopped
    } else if let Some(days) = cleanup_matches.get_one::<u64>("older-than") {
        cleanup::CleanupStrategy::OlderThan(*days)
    } else {
        cleanup::CleanupStrategy::All
    };

    match cleanup::cleanup_all_with_strategy(strategy) {
```

**Why**: Wire CLI flags to CleanupStrategy enum variants

---

### Step 2: Add cleanup_all_with_strategy function

**File**: `src/cleanup/handler.rs`
**Lines**: NEW
**Action**: UPDATE

**Required change:**
```rust
pub fn cleanup_all_with_strategy(strategy: CleanupStrategy) -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.cleanup_all_with_strategy_started", strategy = ?strategy);

    // First scan for orphaned resources with strategy
    let scan_summary = scan_for_orphans_with_strategy(strategy)?;

    if scan_summary.total_cleaned == 0 {
        info!(event = "cleanup.cleanup_all_with_strategy_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    // Then clean them up
    let cleanup_summary = cleanup_orphaned_resources(&scan_summary)?;

    info!(
        event = "cleanup.cleanup_all_with_strategy_completed",
        total_cleaned = cleanup_summary.total_cleaned
    );

    Ok(cleanup_summary)
}
```

**Why**: Provide strategy-aware cleanup entry point

---

### Step 3: Add scan_for_orphans_with_strategy function

**File**: `src/cleanup/handler.rs`
**Lines**: NEW
**Action**: UPDATE

**Required change:**
```rust
pub fn scan_for_orphans_with_strategy(strategy: CleanupStrategy) -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.scan_with_strategy_started", strategy = ?strategy);

    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let repo = Repository::discover(&current_dir).map_err(|_| CleanupError::NotInRepository)?;
    let config = Config::new();

    let mut summary = CleanupSummary::new();

    match strategy {
        CleanupStrategy::All => {
            // Use existing scan_for_orphans logic
            return scan_for_orphans();
        }
        CleanupStrategy::NoPid => {
            let sessions = operations::detect_sessions_without_pid(&config.sessions_dir())?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::Stopped => {
            let sessions = operations::detect_sessions_with_stopped_processes(&config.sessions_dir())?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::OlderThan(days) => {
            let sessions = operations::detect_old_sessions(&config.sessions_dir(), days)?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
    }

    info!(
        event = "cleanup.scan_with_strategy_completed",
        total_sessions = summary.stale_sessions.len()
    );

    Ok(summary)
}
```

**Why**: Use existing detection functions based on strategy

---

### Step 4: Export new functions in mod.rs

**File**: `src/cleanup/mod.rs`
**Lines**: 6-9
**Action**: UPDATE

**Current code:**
```rust
pub use handler::{
    cleanup_all, cleanup_orphaned_resources, scan_for_orphans,
};
```

**Required change:**
```rust
pub use handler::{
    cleanup_all, cleanup_all_with_strategy, cleanup_orphaned_resources, 
    scan_for_orphans, scan_for_orphans_with_strategy,
};
```

**Why**: Make new strategy functions available to CLI

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/cleanup/handler.rs:85-95
// Pattern for cleanup function structure and logging
pub fn cleanup_all() -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.cleanup_all_started");

    // First scan for orphaned resources
    let scan_summary = scan_for_orphans()?;

    if scan_summary.total_cleaned == 0 {
        info!(event = "cleanup.cleanup_all_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }
```

```rust
// SOURCE: src/cli/commands.rs:301-315
// Pattern for error handling and user output
match cleanup::cleanup_all() {
    Ok(summary) => {
        println!("✅ Cleanup completed successfully!");
        
        if summary.total_cleaned > 0 {
            println!("   Resources cleaned:");
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Multiple flags specified | Use if-else chain to prioritize: no-pid > stopped > older-than > all |
| Invalid days value | Clap parser handles validation with value_parser!(u64) |
| Strategy functions fail | Existing error handling in operations.rs covers this |

---

## Validation

### Automated Checks

```bash
cargo check
cargo test cleanup
cargo clippy
```

### Manual Verification

1. Run `shards cleanup --no-pid` and verify it only cleans sessions without PID
2. Run `shards cleanup --stopped` and verify it only cleans stopped processes
3. Run `shards cleanup --older-than 7` and verify it only cleans old sessions
4. Verify no build warnings for unused functions

---

## Scope Boundaries

**IN SCOPE:**
- Wire existing CLI flags to existing detection functions
- Add missing handler functions for strategy-based cleanup
- Export new functions in module

**OUT OF SCOPE (do not touch):**
- Modify existing detection function logic
- Change CLI flag definitions
- Alter existing cleanup_all() behavior

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-20T15:13:43.605+02:00
- **Artifact**: `.archon/artifacts/issues/issue-27.md`
