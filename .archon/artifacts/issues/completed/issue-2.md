# Investigation: Add Ghostty terminal support (blocked by upstream -e parsing)

**Issue**: #2 (https://github.com/Wirasm/shards/issues/2)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-21T12:34:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | HIGH | Upstream blocker is now RESOLVED (Ghostty issue #7032 closed April 2025), Ghostty support is partially implemented but uses fragile AppleScript workaround instead of direct `-e` execution |
| Complexity | LOW | Changes required are minimal: update AppleScript to use `direct:` prefix syntax, update config validation, update error messages - all in 2-3 files |
| Confidence | HIGH | Upstream fix is confirmed merged (PR #7044, Ghostty 1.2.0), current implementation paths are well understood, clear evidence of required changes |

---

## Problem Statement

Ghostty terminal support was implemented using AppleScript keystroke automation as a workaround for Ghostty's `-e` flag not working correctly. The upstream issue (ghostty-org/ghostty#7032) has been **resolved and merged** as of April 2025 in Ghostty 1.2.0. The codebase can now be updated to use proper direct command execution via `ghostty -e direct:command` instead of the fragile AppleScript workaround.

---

## Analysis

### Current State vs Desired State

**Current implementation** (`src/terminal/operations.rs:17-28`):
```rust
const GHOSTTY_SCRIPT: &str = r#"try
        tell application "Ghostty"
            activate
            delay 0.5
        end tell
        tell application "System Events"
            keystroke "{}"
            keystroke return
        end tell
    on error errMsg
        error "Failed to launch Ghostty: " & errMsg
    end try"#;
```

**Problems with current approach**:
1. Uses AppleScript System Events keystroke simulation - fragile and slow
2. Requires `delay 0.5` for window to be ready
3. Can fail if Ghostty window doesn't have focus
4. No proper error handling for command execution

**Desired state**: Use Ghostty's now-fixed `-e` flag with `direct:` prefix:
```bash
ghostty -e direct:fish -c "cd /path && command"
```

### Upstream Resolution Details

**Ghostty Issue #7032** - CLOSED/COMPLETED (April 10, 2025):
- PR #7044 merged introducing `direct:` and `shell:` prefix syntax
- `direct:` - Commands execute via `execvpe()` without shell expansion
- `shell:` - Commands run through `/bin/sh -c` with full shell expansion
- `-e` flag now defaults to `direct:` mode
- Released in Ghostty 1.2.0 (milestone closed September 15, 2025)

### Evidence Chain

WHY: Current Ghostty implementation uses fragile AppleScript workaround
   Evidence: `src/terminal/operations.rs:17-28` - keystroke simulation

BECAUSE: Ghostty `-e` flag didn't parse arguments correctly
   Evidence: Issue #2 body - "runs in background without visible windows"

BECAUSE: Upstream Ghostty had a parsing bug
   Evidence: https://github.com/ghostty-org/ghostty/issues/7032

ROOT CAUSE: Upstream bug - **NOW RESOLVED**
   Evidence: PR #7044 merged April 10, 2025, released in v1.2.0

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/terminal/operations.rs` | 17-28, 84-88 | UPDATE | Replace AppleScript with direct `-e` command execution |
| `src/terminal/errors.rs` | 5 | UPDATE | Update error message to include Ghostty |
| `src/core/config.rs` | 150 | UPDATE | Add "ghostty" and "native" to valid_terminals list |
| `src/terminal/operations.rs` | 157-195 | UPDATE | Add test for Ghostty command building |

### Integration Points

- `src/terminal/handler.rs:78-107` - Calls `build_spawn_command()` for terminal spawning
- `src/sessions/handler.rs:84-87` - Uses terminal spawning for session creation
- `src/cli/commands.rs:54-56` - Passes `--terminal ghostty` CLI override

### Git History

- **Current implementation**: AppleScript workaround added for Ghostty support
- **Upstream fix**: Ghostty PR #7044 merged April 10, 2025
- **Implication**: Can now use proper command-line execution

---

## Implementation Plan

### Step 1: Update Ghostty command execution to use direct `-e` flag

**File**: `src/terminal/operations.rs`
**Lines**: 17-28, 84-88
**Action**: UPDATE

**Current code:**
```rust
// Lines 17-28
const GHOSTTY_SCRIPT: &str = r#"try
        tell application "Ghostty"
            activate
            delay 0.5
        end tell
        tell application "System Events"
            keystroke "{}"
            keystroke return
        end tell
    on error errMsg
        error "Failed to launch Ghostty: " & errMsg
    end try"#;

// Lines 84-88
TerminalType::Ghostty => Ok(vec![
    "osascript".to_string(),
    "-e".to_string(),
    GHOSTTY_SCRIPT.replace("{}", &applescript_escape(&cd_command)),
]),
```

**Required change:**
```rust
// Remove GHOSTTY_SCRIPT constant entirely (lines 17-28)

// Lines 84-88 - Replace with direct command execution
TerminalType::Ghostty => {
    // Use Ghostty's direct execution mode (fixed in Ghostty 1.2.0)
    // The direct: prefix ensures execvpe() execution without shell expansion
    let shell_command = format!(
        "cd {} && {}",
        shell_escape(&config.working_directory.display().to_string()),
        config.command
    );
    Ok(vec![
        "ghostty".to_string(),
        "-e".to_string(),
        format!("direct:fish -c {}", shell_escape(&shell_command)),
    ])
},
```

**Why**: Ghostty's `-e` flag is now fixed with the `direct:` prefix syntax, allowing proper command execution without the fragile AppleScript workaround.

---

### Step 2: Update error message to include Ghostty

**File**: `src/terminal/errors.rs`
**Lines**: 5
**Action**: UPDATE

**Current code:**
```rust
#[error("No supported terminal found (tried: iTerm, Terminal.app)")]
NoTerminalFound,
```

**Required change:**
```rust
#[error("No supported terminal found (tried: Ghostty, iTerm, Terminal.app)")]
NoTerminalFound,
```

**Why**: Ghostty is checked first in detection order, so it should be listed in the error message.

---

### Step 3: Update config validation to include ghostty and native

**File**: `src/core/config.rs`
**Lines**: 150
**Action**: UPDATE

**Current code:**
```rust
let valid_terminals = ["iterm2", "iterm", "terminal"];
```

**Required change:**
```rust
let valid_terminals = ["iterm2", "iterm", "terminal", "ghostty", "native"];
```

**Why**: Config validation should accept "ghostty" and "native" as valid terminal preferences since they are supported terminal types.

---

### Step 4: Add/Update tests for Ghostty command building

**File**: `src/terminal/operations.rs`
**Lines**: After line 195
**Action**: UPDATE

**Test cases to add:**
```rust
#[test]
fn test_build_spawn_command_ghostty() {
    let config = SpawnConfig::new(
        TerminalType::Ghostty,
        std::env::current_dir().unwrap(),
        "claude".to_string(),
    );

    let result = build_spawn_command(&config);
    assert!(result.is_ok());

    let command = result.unwrap();
    assert_eq!(command[0], "ghostty");
    assert_eq!(command[1], "-e");
    assert!(command[2].starts_with("direct:"));
    assert!(command[2].contains("claude"));
}

#[test]
fn test_build_spawn_command_ghostty_with_spaces() {
    let config = SpawnConfig::new(
        TerminalType::Ghostty,
        std::env::current_dir().unwrap(),
        "kiro-cli chat --verbose".to_string(),
    );

    let result = build_spawn_command(&config);
    assert!(result.is_ok());

    let command = result.unwrap();
    assert!(command[2].contains("kiro-cli chat --verbose"));
}
```

---

### Step 5: Update test assertion for error message

**File**: `src/terminal/errors.rs`
**Lines**: 66-69
**Action**: UPDATE

**Current code:**
```rust
assert_eq!(
    error.to_string(),
    "No supported terminal found (tried: iTerm, Terminal.app)"
);
```

**Required change:**
```rust
assert_eq!(
    error.to_string(),
    "No supported terminal found (tried: Ghostty, iTerm, Terminal.app)"
);
```

**Why**: Test must match updated error message.

---

## Patterns to Follow

**From codebase - mirror the iTerm/Terminal.app patterns:**

```rust
// SOURCE: src/terminal/operations.rs:74-78
// Pattern for iTerm command building
TerminalType::ITerm => Ok(vec![
    "osascript".to_string(),
    "-e".to_string(),
    ITERM_SCRIPT.replace("{}", &applescript_escape(&cd_command)),
]),
```

For Ghostty, instead of AppleScript, use direct command execution since the upstream fix now supports it.

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| User has Ghostty < 1.2.0 without fix | Document minimum version requirement; AppleScript fallback is complex - recommend upgrade instead |
| fish shell not installed | Use `sh -c` instead of `fish -c` for broader compatibility |
| Command with special characters | Use `shell_escape()` function already in codebase |
| Working directory with spaces | Already handled by `shell_escape()` |
| Ghostty not in PATH | Use `open -na Ghostty.app --args` pattern as fallback |

---

## Validation

### Automated Checks

```bash
cargo check
cargo test terminal
cargo clippy
```

### Manual Verification

1. Install Ghostty 1.2.0 or later
2. Run `shards create test-branch --terminal ghostty`
3. Verify terminal window opens with correct working directory
4. Verify command executes properly
5. Test with commands containing spaces and special characters
6. Test auto-detection with Ghostty installed (should be detected first)

---

## Scope Boundaries

**IN SCOPE:**
- Replace AppleScript workaround with direct `-e` execution
- Update error message to include Ghostty
- Update config validation to accept "ghostty" and "native"
- Add tests for Ghostty command building

**OUT OF SCOPE (do not touch):**
- iTerm or Terminal.app implementations (working correctly)
- Linux/Windows terminal support (not implemented yet)
- Ghostty version detection (users should update)
- Process tracking logic (already works for any terminal)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-21T12:34:00Z
- **Artifact**: `.archon/artifacts/issues/issue-2.md`
