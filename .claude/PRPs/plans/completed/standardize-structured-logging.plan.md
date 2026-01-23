# Feature: Standardize Structured Logging with Layer-Prefixed Event Names

## Summary

Standardize all 213 structured logging events in the `shards-core` crate to follow a consistent `{layer}.{domain}.{action}_{state}` naming convention by adding the `core.` prefix. This enables better observability, filtering, and future TUI/dashboard integration. The CLI layer events (28 total) already have the correct `cli.` prefix.

## User Story

As a **developer/operator monitoring shards**
I want to **filter logs by layer (cli vs core) using consistent event prefixes**
So that **I can easily debug issues, aggregate metrics, and build observability dashboards**

## Problem Statement

Core library events lack the `core.` layer prefix, making it impossible to:
1. Filter by layer - `grep "core\."` doesn't work because core modules don't have the prefix
2. Distinguish CLI events from core library events in log aggregation
3. Build layer-aware dashboards for the future TUI

## Solution Statement

Add `core.` prefix to all 213 event names in `shards-core` crate files. This is a mechanical string replacement task across 19 files, with no logic changes required.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | REFACTOR                                          |
| Complexity       | LOW                                               |
| Systems Affected | shards-core (logging only)                        |
| Dependencies     | tracing 0.1 (already in use)                      |
| Estimated Tasks  | 19 (one per file)                                 |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   Log Output Example:                                                         ║
║   ┌─────────────────────────────────────────────────────────────────────┐     ║
║   │ INFO  event="cli.create_started" branch="feature-x"                 │     ║
║   │ INFO  event="session.create_started" branch="feature-x"             │     ║
║   │ INFO  event="git.worktree.create_started" branch="feature-x"        │     ║
║   │ INFO  event="terminal.spawn_started" command="claude"               │     ║
║   └─────────────────────────────────────────────────────────────────────┘     ║
║                                                                               ║
║   Filtering Capability:                                                       ║
║   • grep "cli\."     → ✅ Returns CLI events                                  ║
║   • grep "core\."    → ❌ Returns NOTHING (no core prefix exists)             ║
║   • grep "session\." → ⚠️  Returns session events (but layer unclear)         ║
║                                                                               ║
║   PAIN_POINT: Cannot distinguish CLI vs core library events                   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   Log Output Example:                                                         ║
║   ┌─────────────────────────────────────────────────────────────────────┐     ║
║   │ INFO  event="cli.create_started" branch="feature-x"                 │     ║
║   │ INFO  event="core.session.create_started" branch="feature-x"        │     ║
║   │ INFO  event="core.git.worktree.create_started" branch="feature-x"   │     ║
║   │ INFO  event="core.terminal.spawn_started" command="claude"          │     ║
║   └─────────────────────────────────────────────────────────────────────┘     ║
║                                                                               ║
║   Filtering Capability:                                                       ║
║   • grep "cli\."           → ✅ Returns all CLI events (28)                   ║
║   • grep "core\."          → ✅ Returns all core events (213)                 ║
║   • grep "core\.session\." → ✅ Returns core session events only              ║
║   • grep "_failed"         → ✅ Returns all failures (any layer)              ║
║   • grep "_started"        → ✅ Returns all operation starts                  ║
║                                                                               ║
║   VALUE_ADD: Layer-aware filtering for debugging and observability            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| Log output | `session.create_started` | `core.session.create_started` | Can filter by layer |
| Log aggregation | Mixed prefixes | Consistent `core.` + domain | Better dashboards |
| Debugging | Unclear event source | Layer prefix identifies crate | Faster issue triage |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards/src/commands.rs` | 1-50 | Pattern reference: CLI events with `cli.` prefix |
| P0 | Issue #65 description | all | Full specification and rename mapping |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [tracing docs](https://docs.rs/tracing/0.1) | Structured fields | Verify event field syntax (already in use) |

---

## Patterns to Mirror

**EVENT_NAMING_CONVENTION:**
```rust
// SOURCE: crates/shards/src/commands.rs:70-73
// CORRECT PATTERN (CLI layer):
info!(
    event = "cli.create_started",
    branch = branch,
    agent = config.agent.default
);

// TARGET PATTERN (Core layer):
info!(
    event = "core.session.create_started",  // Add "core." prefix
    branch = request.branch,
    agent = agent,
    command = agent_command
);
```

**BEFORE/AFTER EXAMPLES:**
```rust
// Session events
"session.create_started"         → "core.session.create_started"
"session.list_completed"         → "core.session.list_completed"

// Terminal events
"terminal.spawn_started"         → "core.terminal.spawn_started"
"terminal.close_completed"       → "core.terminal.close_completed"

// Git events (already have sub-domain, add layer prefix)
"git.project.detect_started"     → "core.git.project.detect_started"
"git.worktree.create_completed"  → "core.git.worktree.create_completed"

// Health events
"health.get_all_started"         → "core.health.get_all_started"

// Cleanup events
"cleanup.scan_started"           → "core.cleanup.scan_started"

// Files events
"files.copy.started"             → "core.files.copy.started"

// Process/PID events
"pid_file.read_success"          → "core.pid_file.read_success"
"process.agent_patterns_found"   → "core.process.agent_patterns_found"

// App events
"app.startup_completed"          → "core.app.startup_completed"
```

---

## Files to Change

| File | Action | Event Count | Justification |
|------|--------|-------------|---------------|
| `crates/shards-core/src/sessions/handler.rs` | UPDATE | 37 | Session management events |
| `crates/shards-core/src/sessions/persistence.rs` | UPDATE | 6 | Session file I/O events |
| `crates/shards-core/src/terminal/handler.rs` | UPDATE | 23 | Terminal spawn/close events |
| `crates/shards-core/src/terminal/registry.rs` | UPDATE | 4 | Terminal detection events |
| `crates/shards-core/src/terminal/operations.rs` | UPDATE | 2 | Terminal script events |
| `crates/shards-core/src/terminal/backends/ghostty.rs` | UPDATE | 9 | Ghostty-specific events |
| `crates/shards-core/src/terminal/backends/iterm.rs` | UPDATE | 3 | iTerm-specific events |
| `crates/shards-core/src/terminal/backends/terminal_app.rs` | UPDATE | 3 | Terminal.app events |
| `crates/shards-core/src/terminal/common/applescript.rs` | UPDATE | 6 | AppleScript execution events |
| `crates/shards-core/src/cleanup/handler.rs` | UPDATE | 43 | Cleanup operation events |
| `crates/shards-core/src/cleanup/operations.rs` | UPDATE | 9 | Cleanup helper events |
| `crates/shards-core/src/git/handler.rs` | UPDATE | 24 | Git worktree/branch events |
| `crates/shards-core/src/health/handler.rs` | UPDATE | 6 | Health check events |
| `crates/shards-core/src/health/storage.rs` | UPDATE | 9 | Health history events |
| `crates/shards-core/src/files/handler.rs` | UPDATE | 12 | File copy events |
| `crates/shards-core/src/files/operations.rs` | UPDATE | 5 | File matching events |
| `crates/shards-core/src/process/pid_file.rs` | UPDATE | 7 | PID file events |
| `crates/shards-core/src/process/operations.rs` | UPDATE | 2 | Process search events |
| `crates/shards-core/src/events/mod.rs` | UPDATE | 3 | App lifecycle events |

**Total: 19 files, 213 event renames**

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No changes to log levels** - info/debug/error levels stay as-is
- **No changes to structured payload fields** - only the event name string changes
- **No changes to tracing-subscriber configuration** - subscriber setup unchanged
- **No new dependencies** - uses existing tracing crate
- **No CLI layer changes** - `crates/shards/` events already have `cli.` prefix

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `crates/shards-core/src/sessions/handler.rs`

- **ACTION**: Add `core.` prefix to all 37 session events
- **IMPLEMENT**: Find/replace `event = "session.` → `event = "core.session.`
- **EVENTS TO RENAME**:
  - `session.agent_not_available` → `core.session.agent_not_available`
  - `session.create_started` → `core.session.create_started`
  - `session.project_detected` → `core.session.project_detected`
  - `session.port_allocation_failed` → `core.session.port_allocation_failed`
  - `session.port_allocated` → `core.session.port_allocated`
  - `session.worktree_created` → `core.session.worktree_created`
  - `session.create_completed` → `core.session.create_completed`
  - `session.list_started` → `core.session.list_started`
  - `session.list_skipped_sessions` → `core.session.list_skipped_sessions`
  - `session.list_completed` → `core.session.list_completed`
  - `session.get_started` → `core.session.get_started`
  - `session.get_completed` → `core.session.get_completed`
  - `session.destroy_*` (multiple) → `core.session.destroy_*`
  - `session.port_deallocated` → `core.session.port_deallocated`
  - `session.restart_*` (multiple) → `core.session.restart_*`
- **VALIDATE**: `cargo check -p shards-core`

### Task 2: UPDATE `crates/shards-core/src/sessions/persistence.rs`

- **ACTION**: Add `core.` prefix to 6 session persistence events
- **IMPLEMENT**: Find/replace `event = "session.` → `event = "core.session.`
- **EVENTS TO RENAME**:
  - `session.temp_file_cleanup_failed` → `core.session.temp_file_cleanup_failed`
  - `session.serialization_failed` → `core.session.serialization_failed`
  - `session.load_read_error` → `core.session.load_read_error`
  - `session.load_invalid_json` → `core.session.load_invalid_json`
  - `session.load_invalid_structure` → `core.session.load_invalid_structure`
  - `session.remove_nonexistent_file` → `core.session.remove_nonexistent_file`
- **VALIDATE**: `cargo check -p shards-core`

### Task 3: UPDATE `crates/shards-core/src/terminal/handler.rs`

- **ACTION**: Add `core.` prefix to 23 terminal handler events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.searching_for_agent_process` → `core.terminal.searching_for_agent_process`
  - `terminal.agent_process_found` → `core.terminal.agent_process_found`
  - `terminal.agent_process_not_found_*` → `core.terminal.agent_process_not_found_*`
  - `terminal.spawn_started` → `core.terminal.spawn_started`
  - `terminal.spawn_completed` → `core.terminal.spawn_completed`
  - `terminal.detect_*` → `core.terminal.detect_*`
  - `terminal.pid_file_*` → `core.terminal.pid_file_*`
  - `terminal.close_*` → `core.terminal.close_*`
  - `terminal.unknown_preference` → `core.terminal.unknown_preference`
  - `terminal.command_wrapped` → `core.terminal.command_wrapped`
  - `terminal.spawn_script_executed` → `core.terminal.spawn_script_executed`
  - `terminal.reading_pid_file` → `core.terminal.reading_pid_file`
- **VALIDATE**: `cargo check -p shards-core`

### Task 4: UPDATE `crates/shards-core/src/terminal/registry.rs`

- **ACTION**: Add `core.` prefix to 4 terminal registry events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.detection_started` → `core.terminal.detection_started`
  - `terminal.detected` → `core.terminal.detected`
  - `terminal.none_found` → `core.terminal.none_found`
  - `terminal.platform_not_supported` → `core.terminal.platform_not_supported`
- **VALIDATE**: `cargo check -p shards-core`

### Task 5: UPDATE `crates/shards-core/src/terminal/operations.rs`

- **ACTION**: Add `core.` prefix to 2 terminal operations events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.spawn_script_not_supported` → `core.terminal.spawn_script_not_supported`
  - `terminal.close_not_supported` → `core.terminal.close_not_supported`
- **VALIDATE**: `cargo check -p shards-core`

### Task 6: UPDATE `crates/shards-core/src/terminal/backends/ghostty.rs`

- **ACTION**: Add `core.` prefix to 9 Ghostty backend events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.spawn_ghostty_starting` → `core.terminal.spawn_ghostty_starting`
  - `terminal.spawn_ghostty_launched` → `core.terminal.spawn_ghostty_launched`
  - `terminal.spawn_ghostty_not_supported` → `core.terminal.spawn_ghostty_not_supported`
  - `terminal.close_skipped_no_id` → `core.terminal.close_skipped_no_id`
  - `terminal.close_ghostty_pkill` → `core.terminal.close_ghostty_pkill`
  - `terminal.close_ghostty_completed` → `core.terminal.close_ghostty_completed`
  - `terminal.close_ghostty_no_match` → `core.terminal.close_ghostty_no_match`
  - `terminal.close_ghostty_failed` → `core.terminal.close_ghostty_failed`
  - `terminal.close_not_supported` → `core.terminal.close_not_supported`
- **VALIDATE**: `cargo check -p shards-core`

### Task 7: UPDATE `crates/shards-core/src/terminal/backends/iterm.rs`

- **ACTION**: Add `core.` prefix to 3 iTerm backend events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.spawn_iterm_not_supported` → `core.terminal.spawn_iterm_not_supported`
  - `terminal.close_skipped_no_id` → `core.terminal.close_skipped_no_id`
  - `terminal.close_not_supported` → `core.terminal.close_not_supported`
- **VALIDATE**: `cargo check -p shards-core`

### Task 8: UPDATE `crates/shards-core/src/terminal/backends/terminal_app.rs`

- **ACTION**: Add `core.` prefix to 3 Terminal.app backend events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.spawn_terminal_app_not_supported` → `core.terminal.spawn_terminal_app_not_supported`
  - `terminal.close_skipped_no_id` → `core.terminal.close_skipped_no_id`
  - `terminal.close_not_supported` → `core.terminal.close_not_supported`
- **VALIDATE**: `cargo check -p shards-core`

### Task 9: UPDATE `crates/shards-core/src/terminal/common/applescript.rs`

- **ACTION**: Add `core.` prefix to 6 AppleScript events
- **IMPLEMENT**: Find/replace `event = "terminal.` → `event = "core.terminal.`
- **EVENTS TO RENAME**:
  - `terminal.applescript_executing` → `core.terminal.applescript_executing`
  - `terminal.applescript_completed` → `core.terminal.applescript_completed`
  - `terminal.close_started` → `core.terminal.close_started`
  - `terminal.close_completed` → `core.terminal.close_completed`
  - `terminal.close_failed` → `core.terminal.close_failed`
- **VALIDATE**: `cargo check -p shards-core`

### Task 10: UPDATE `crates/shards-core/src/cleanup/handler.rs`

- **ACTION**: Add `core.` prefix to 43 cleanup handler events
- **IMPLEMENT**: Find/replace `event = "cleanup.` → `event = "core.cleanup.`
- **EVENTS TO RENAME**:
  - `cleanup.scan_*` → `core.cleanup.scan_*`
  - `cleanup.cleanup_*` → `core.cleanup.cleanup_*`
  - `cleanup.git_discovery_failed` → `core.cleanup.git_discovery_failed`
  - `cleanup.strategy_*` → `core.cleanup.strategy_*`
  - `cleanup.orphans_scan_completed` → `core.cleanup.orphans_scan_completed`
  - `cleanup.branch_*` → `core.cleanup.branch_*`
  - `cleanup.worktree_*` → `core.cleanup.worktree_*`
  - `cleanup.session_*` → `core.cleanup.session_*`
- **VALIDATE**: `cargo check -p shards-core`

### Task 11: UPDATE `crates/shards-core/src/cleanup/operations.rs`

- **ACTION**: Add `core.` prefix to 9 cleanup operations events
- **IMPLEMENT**: Find/replace `event = "cleanup.` → `event = "core.cleanup.`
- **EVENTS TO RENAME**:
  - `cleanup.worktree_find_failed` → `core.cleanup.worktree_find_failed`
  - `cleanup.worktree_canonicalize_failed` → `core.cleanup.worktree_canonicalize_failed`
  - `cleanup.project_dir_canonicalize_failed` → `core.cleanup.project_dir_canonicalize_failed`
  - `cleanup.session_invalid_worktree_path_type` → `core.cleanup.session_invalid_worktree_path_type`
  - `cleanup.session_missing_worktree_path` → `core.cleanup.session_missing_worktree_path`
  - `cleanup.session_json_parse_failed` → `core.cleanup.session_json_parse_failed`
  - `cleanup.session_read_failed` → `core.cleanup.session_read_failed`
  - `cleanup.malformed_session_file` → `core.cleanup.malformed_session_file`
  - `cleanup.unreadable_session_file` → `core.cleanup.unreadable_session_file`
- **VALIDATE**: `cargo check -p shards-core`

### Task 12: UPDATE `crates/shards-core/src/git/handler.rs`

- **ACTION**: Add `core.` prefix to 24 git handler events
- **IMPLEMENT**: Find/replace `event = "git.` → `event = "core.git.`
- **EVENTS TO RENAME**:
  - `git.project.detect_started` → `core.git.project.detect_started`
  - `git.project.detect_completed` → `core.git.project.detect_completed`
  - `git.worktree.*` → `core.git.worktree.*`
  - `git.branch.*` → `core.git.branch.*`
- **VALIDATE**: `cargo check -p shards-core`

### Task 13: UPDATE `crates/shards-core/src/health/handler.rs`

- **ACTION**: Add `core.` prefix to 6 health handler events
- **IMPLEMENT**: Find/replace `event = "health.` → `event = "core.health.`
- **EVENTS TO RENAME**:
  - `health.get_all_started` → `core.health.get_all_started`
  - `health.get_all_completed` → `core.health.get_all_completed`
  - `health.get_single_started` → `core.health.get_single_started`
  - `health.get_single_completed` → `core.health.get_single_completed`
  - `health.process_metrics_failed` → `core.health.process_metrics_failed`
  - `health.process_check_failed` → `core.health.process_check_failed`
- **VALIDATE**: `cargo check -p shards-core`

### Task 14: UPDATE `crates/shards-core/src/health/storage.rs`

- **ACTION**: Add `core.` prefix to 9 health storage events
- **IMPLEMENT**: Find/replace `event = "health.` → `event = "core.health.`
- **EVENTS TO RENAME**:
  - `health.history_parse_failed` → `core.health.history_parse_failed`
  - `health.history_file_parse_failed` → `core.health.history_file_parse_failed`
  - `health.history_file_read_failed` → `core.health.history_file_read_failed`
  - `health.history_dir_entry_failed` → `core.health.history_dir_entry_failed`
  - `health.history_dir_read_failed` → `core.health.history_dir_read_failed`
  - `health.history_cleanup_delete_failed` → `core.health.history_cleanup_delete_failed`
  - `health.history_cleanup_entry_failed` → `core.health.history_cleanup_entry_failed`
  - `health.history_cleanup_dir_read_failed` → `core.health.history_cleanup_dir_read_failed`
  - `health.history_cleanup_partial` → `core.health.history_cleanup_partial`
- **VALIDATE**: `cargo check -p shards-core`

### Task 15: UPDATE `crates/shards-core/src/files/handler.rs`

- **ACTION**: Add `core.` prefix to 12 files handler events
- **IMPLEMENT**: Find/replace `event = "files.` → `event = "core.files.`
- **EVENTS TO RENAME**:
  - `files.copy.started` → `core.files.copy.started`
  - `files.copy.skipped` → `core.files.copy.skipped`
  - `files.copy.failed` → `core.files.copy.failed`
  - `files.copy.completed` → `core.files.copy.completed`
  - `files.copy.warning` → `core.files.copy.warning`
  - `files.copy.file_completed` → `core.files.copy.file_completed`
  - `files.copy.file_failed` → `core.files.copy.file_failed`
  - `files.copy.completed_with_errors` → `core.files.copy.completed_with_errors`
- **VALIDATE**: `cargo check -p shards-core`

### Task 16: UPDATE `crates/shards-core/src/files/operations.rs`

- **ACTION**: Add `core.` prefix to 5 files operations events
- **IMPLEMENT**: Find/replace `event = "files.` → `event = "core.files.`
- **EVENTS TO RENAME**:
  - `files.patterns.validated` → `core.files.patterns.validated`
  - `files.pattern.matched` → `core.files.pattern.matched`
  - `files.walk.error` → `core.files.walk.error`
  - `files.matching.completed` → `core.files.matching.completed`
  - `files.copy.completed` → `core.files.copy.completed`
- **VALIDATE**: `cargo check -p shards-core`

### Task 17: UPDATE `crates/shards-core/src/process/pid_file.rs`

- **ACTION**: Add `core.` prefix to 7 PID file events
- **IMPLEMENT**: Find/replace `event = "pid_file.` → `event = "core.pid_file.`
- **EVENTS TO RENAME**:
  - `pid_file.dir_created` → `core.pid_file.dir_created`
  - `pid_file.read_attempt` → `core.pid_file.read_attempt`
  - `pid_file.read_success` → `core.pid_file.read_success`
  - `pid_file.not_found_retry` → `core.pid_file.not_found_retry`
  - `pid_file.read_error` → `core.pid_file.read_error`
  - `pid_file.not_found_final` → `core.pid_file.not_found_final`
  - `pid_file.deleted` → `core.pid_file.deleted`
- **VALIDATE**: `cargo check -p shards-core`

### Task 18: UPDATE `crates/shards-core/src/process/operations.rs`

- **ACTION**: Add `core.` prefix to 2 process operations events
- **IMPLEMENT**: Find/replace `event = "process.` → `event = "core.process.`
- **EVENTS TO RENAME**:
  - `process.agent_patterns_found` → `core.process.agent_patterns_found`
  - `process.agent_patterns_not_found` → `core.process.agent_patterns_not_found`
- **VALIDATE**: `cargo check -p shards-core`

### Task 19: UPDATE `crates/shards-core/src/events/mod.rs`

- **ACTION**: Add `core.` prefix to 3 app lifecycle events
- **IMPLEMENT**: Find/replace `event = "app.` → `event = "core.app.`
- **EVENTS TO RENAME**:
  - `app.startup_completed` → `core.app.startup_completed`
  - `app.shutdown_started` → `core.app.shutdown_started`
  - `app.error_occurred` → `core.app.error_occurred`
- **VALIDATE**: `cargo check -p shards-core`

### Task 20: UPDATE `CONTRIBUTING.md`

- **ACTION**: Add logging convention documentation
- **IMPLEMENT**: Add a section documenting the event naming convention
- **CONTENT**:
```markdown
## Structured Logging Convention

All log events follow the pattern: `{layer}.{domain}.{action}_{state}`

| Layer | Crate | Description |
|-------|-------|-------------|
| `cli` | `crates/shards/` | User-facing CLI commands |
| `core` | `crates/shards-core/` | Core library logic |

Examples:
- `cli.create_started` - CLI layer starting create command
- `core.session.create_completed` - Core session creation succeeded
- `core.terminal.spawn_failed` - Core terminal spawn failed
```
- **VALIDATE**: Visual inspection

### Task 21: VERIFY - Full validation

- **ACTION**: Run full test suite and verify no events without layer prefix
- **IMPLEMENT**:
  ```bash
  # Full build and test
  cargo build --all
  cargo test --all
  cargo clippy --all

  # Verify all events have layer prefix
  grep -r 'event = "' crates/ | grep -v 'core\.\|cli\.'
  # Should return empty (all events have layer prefix)
  ```
- **VALIDATE**: All commands pass with exit 0

---

## Testing Strategy

### Unit Tests to Write

No new tests required - this is a string replacement refactor with no behavior changes.

### Verification Tests

| Test | Command | Expected Result |
|------|---------|-----------------|
| Build succeeds | `cargo build --all` | Exit 0 |
| Tests pass | `cargo test --all` | All tests pass |
| Clippy clean | `cargo clippy --all` | No warnings |
| All events prefixed | `grep -r 'event = "' crates/ \| grep -v 'core\.\|cli\.'` | Empty output |

### Edge Cases Checklist

- [x] No behavior changes - string replacement only
- [x] No new dependencies
- [x] No configuration changes
- [x] CLI events already have correct prefix (28 events)
- [x] All core events need prefix (213 events)

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test --all
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: EVENT_PREFIX_VALIDATION

```bash
grep -r 'event = "' crates/ | grep -v 'core\.\|cli\.'
```

**EXPECT**: Empty output (all events have layer prefix)

---

## Acceptance Criteria

- [x] All 213 events in `shards-core` prefixed with `core.`
- [x] All 28 events in `shards` (CLI) prefixed with `cli.` (already done)
- [x] Consistent 3+ level naming: `{layer}.{domain}.{action}_{state}`
- [x] No orphaned 2-level names like `session.created`
- [x] `cargo build --all` succeeds
- [x] `cargo test --all` passes
- [x] Convention documented in `CONTRIBUTING.md`

---

## Completion Checklist

- [ ] All 19 core files updated with `core.` prefix
- [ ] Each task validated immediately after completion with `cargo check -p shards-core`
- [ ] Level 1: Static analysis (fmt + clippy) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full build succeeds
- [ ] Level 4: Event prefix validation returns empty
- [ ] CONTRIBUTING.md updated with convention
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Typo in event name | LOW | LOW | Mechanical find/replace, validated by grep check |
| Missed event | LOW | LOW | Grep validation catches orphaned events |
| Test assertion on event name | LOW | MED | Search for event strings in test files (none found) |

---

## Notes

**Implementation Strategy**:
- Use find/replace in each file: `event = "{domain}.` → `event = "core.{domain}.`
- Validate after each file with `cargo check -p shards-core`
- Final validation with grep to catch any missed events

**Why this is LOW complexity**:
- No logic changes - string replacement only
- No new code paths
- No new dependencies
- No configuration changes
- All changes are in a single crate
- Mechanical, repeatable process
