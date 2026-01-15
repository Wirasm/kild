# Investigation: Extra empty terminal window created during shard creation

**Issue**: #16 (https://github.com/Wirasm/shards/issues/16)
**Type**: BUG
**Investigated**: 2026-01-15T14:42:33+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | LOW | Minor annoyance that doesn't break functionality - all agents launch correctly, just creates one extra empty window |
| Complexity | LOW | Single file change (src/terminal/operations.rs), isolated to iTerm2 AppleScript logic, no integration points affected |
| Confidence | HIGH | Clear root cause identified through code inspection and AppleScript behavior analysis - iTerm2 creates default window on launch, then we create another |

---

## Problem Statement

When creating shards on macOS with iTerm2, an extra empty terminal window appears without any agent process running. This occurs because when iTerm2 is not already running, the AppleScript `tell application "iTerm"` command launches iTerm2 (which creates a default window), and then our `create window with default profile` command creates a second window, resulting in one empty window and one with the agent.

---

## Analysis

### Root Cause / Change Rationale

The issue is in the iTerm2 AppleScript generation in `src/terminal/operations.rs`. The current implementation always executes `create window with default profile`, which creates a NEW window every time.

### Evidence Chain

WHY: Extra empty terminal window appears during shard creation
↓ BECAUSE: iTerm2 AppleScript always creates a new window
  Evidence: `src/terminal/operations.rs:32-41` - AppleScript contains `create window with default profile`

↓ BECAUSE: When iTerm2 is not running, `tell application "iTerm"` launches it with a default window, then we create another
  Evidence: iTerm2 behavior - launching the app creates a default window before our script runs

↓ ROOT CAUSE: AppleScript doesn't check if iTerm2 was just launched and reuse the existing window
  Evidence: `src/terminal/operations.rs:37` - Unconditional `create window with default profile` command

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/terminal/operations.rs` | 32-41 | UPDATE | Fix iTerm2 AppleScript to reuse existing window if iTerm2 was just launched |

### Integration Points

- `src/terminal/handler.rs:76` calls `build_spawn_command()` which generates the AppleScript
- `src/sessions/handler.rs:76` calls `terminal::handler::spawn_terminal()` during shard creation
- No other integration points affected - change is isolated to AppleScript generation

### Git History

- **Introduced**: 706d36c - 2026-01-15 - "feat: Implement file-based persistence system with comprehensive fixes"
- **Last modified**: 706d36c - 2026-01-15
- **Implication**: Recent code, not a regression - original implementation didn't account for iTerm2 launch behavior

---

## Implementation Plan

### Step 1: Fix iTerm2 AppleScript to reuse window when iTerm2 is not running

**File**: `src/terminal/operations.rs`
**Lines**: 32-41
**Action**: UPDATE

**Current code:**
```rust
TerminalType::ITerm => Ok(vec![
    "osascript".to_string(),
    "-e".to_string(),
    format!(
        r#"tell application "iTerm"
                create window with default profile
                tell current session of current window
                    write text "{}"
                end tell
            end tell"#,
        applescript_escape(&cd_command)
    ),
]),
```

**Required change:**
```rust
TerminalType::ITerm => Ok(vec![
    "osascript".to_string(),
    "-e".to_string(),
    format!(
        r#"tell application "iTerm"
                if (count of windows) = 0 then
                    create window with default profile
                end if
                tell current session of current window
                    write text "{}"
                end tell
            end tell"#,
        applescript_escape(&cd_command)
    ),
]),
```

**Why**: Check if iTerm2 has any windows before creating a new one. If iTerm2 was just launched, it will have created a default window (count = 1), so we reuse it. If iTerm2 was already running with windows, we still reuse the current window. This prevents the extra empty window.

---

### Step 2: Verify existing tests still pass

**File**: `src/terminal/operations.rs`
**Lines**: 110-130
**Action**: VERIFY

**Test cases to verify:**
- `test_build_spawn_command_iterm` - Should still pass, just with updated AppleScript
- `test_build_spawn_command_terminal_app` - Unaffected (Terminal.app doesn't have this issue)
- `test_build_spawn_command_empty_command` - Unaffected
- `test_build_spawn_command_nonexistent_directory` - Unaffected

**Why**: Ensure the fix doesn't break existing functionality

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/terminal/operations.rs:43-52
// Pattern for Terminal.app AppleScript generation
TerminalType::TerminalApp => Ok(vec![
    "osascript".to_string(),
    "-e".to_string(),
    format!(
        r#"tell application "Terminal"
                do script "{}"
            end tell"#,
        applescript_escape(&cd_command)
    ),
]),
```

Note: Terminal.app uses `do script` which automatically reuses the current window or creates one if needed, so it doesn't have this issue.

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| iTerm2 already has windows open | AppleScript will reuse current window (existing behavior) |
| iTerm2 not running | AppleScript will detect 0 windows after launch and create one (fixed behavior) |
| Multiple shards created simultaneously | Each will create its own window (desired behavior) |
| User closes iTerm2 between shards | Next shard will launch iTerm2 fresh and reuse the default window (fixed behavior) |

---

## Validation

### Automated Checks

```bash
cargo test --package shards --lib terminal::operations::tests
cargo clippy -- -D warnings
cargo fmt --check
```

### Manual Verification

1. **Quit iTerm2 completely** (Cmd+Q)
2. Run `shards create test-branch --agent kiro`
3. Verify only ONE iTerm2 window opens (not two)
4. Verify the window has the agent running (not empty)
5. Create another shard while iTerm2 is running
6. Verify a new window is created for the second shard

---

## Scope Boundaries

**IN SCOPE:**
- Fix iTerm2 AppleScript to prevent extra empty window
- Maintain existing behavior for Terminal.app
- Ensure tests still pass

**OUT OF SCOPE (do not touch):**
- Process tracking (#13 - separate issue)
- Terminal detection logic
- Other terminal emulators (Linux/Windows)
- Session persistence
- Flag parsing

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-15T14:42:33+02:00
- **Artifact**: `.archon/artifacts/issues/issue-16.md`
