# Investigation: Implement terminal type selection and cross-platform terminal support

**Issue**: #7 (https://github.com/Wirasm/shards/issues/7)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-20T15:14:44.177+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | HIGH | Blocking cross-platform compatibility and user experience - terminal preferences are completely ignored |
| Complexity | MEDIUM | Requires 3-4 files, terminal detection logic, but isolated to terminal module with clear boundaries |
| Confidence | HIGH | Clear root cause identified - missing terminal types and no CLI terminal override support |

---

## Problem Statement

The terminal selection system is incomplete, causing all sessions to launch in iTerm2 regardless of user configuration or intended terminal type. The regression test shows that terminal preferences (ghostty, terminal, native) are ignored, and there's no way to specify terminal type via CLI arguments.

---

## Analysis

### Root Cause / Change Rationale

The terminal system has two critical gaps:

1. **Missing Terminal Types**: Only `ITerm` and `TerminalApp` are defined, but users expect `Ghostty` and cross-platform support
2. **No CLI Terminal Override**: The `--terminal` flag exists in CLI but terminal selection logic doesn't use it properly

### Evidence Chain

WHY: All sessions launch in iTerm2 regardless of configuration
↓ BECAUSE: `detect_terminal()` only checks for iTerm first, then Terminal.app
  Evidence: `src/terminal/operations.rs:4-10` - Only two terminal types supported

↓ BECAUSE: `TerminalType` enum only has `ITerm` and `TerminalApp` variants
  Evidence: `src/terminal/types.rs:4-7` - Missing Ghostty, native, cross-platform types

↓ ROOT CAUSE: Incomplete terminal type system and missing CLI integration
  Evidence: `src/terminal/handler.rs:19-25` - Config terminal preference mapping incomplete

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/terminal/types.rs` | 4-7 | UPDATE | Add Ghostty, Native terminal types |
| `src/terminal/operations.rs` | 4-10, 31-60 | UPDATE | Add detection and spawn logic for new types |
| `src/terminal/handler.rs` | 19-25 | UPDATE | Improve terminal selection logic |
| `src/cli/commands.rs` | 45-50 | UPDATE | Apply CLI terminal override properly |

### Integration Points

- `src/cli/commands.rs:45` applies terminal override to config
- `src/terminal/handler.rs:19` reads config.terminal.preferred
- `src/terminal/operations.rs:4` detects available terminals
- Regression test expects: native, iterm2, ghostty, terminal types

### Git History

- **Introduced**: a19478f - Complete vertical slice architecture implementation
- **Last modified**: 80788d3 - Fix: Extra empty terminal window on iTerm2 launch
- **Implication**: Original implementation was minimal, needs expansion for cross-platform support

---

## Implementation Plan

### Step 1: Extend TerminalType enum with missing variants

**File**: `src/terminal/types.rs`
**Lines**: 4-7
**Action**: UPDATE

**Current code:**
```rust
// Line 4-7
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    ITerm,
    TerminalApp,
}
```

**Required change:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    ITerm,
    TerminalApp,
    Ghostty,
    Native, // System default
}
```

**Why**: Add support for Ghostty and native terminal selection as expected by regression tests

---

### Step 2: Update Display implementation for new terminal types

**File**: `src/terminal/types.rs`
**Lines**: 60-66
**Action**: UPDATE

**Current code:**
```rust
// Line 60-66
impl std::fmt::Display for TerminalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalType::ITerm => write!(f, "iterm"),
            TerminalType::TerminalApp => write!(f, "terminal"),
        }
    }
}
```

**Required change:**
```rust
impl std::fmt::Display for TerminalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalType::ITerm => write!(f, "iterm"),
            TerminalType::TerminalApp => write!(f, "terminal"),
            TerminalType::Ghostty => write!(f, "ghostty"),
            TerminalType::Native => write!(f, "native"),
        }
    }
}
```

**Why**: Ensure string representation matches expected values from regression test

---

### Step 3: Enhance terminal detection logic

**File**: `src/terminal/operations.rs`
**Lines**: 4-10
**Action**: UPDATE

**Current code:**
```rust
// Line 4-10
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    if app_exists_macos("iTerm") {
        Ok(TerminalType::ITerm)
    } else if app_exists_macos("Terminal") {
        Ok(TerminalType::TerminalApp)
    } else {
        Err(TerminalError::NoTerminalFound)
    }
}
```

**Required change:**
```rust
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    // Check for Ghostty first (user preference)
    if app_exists_macos("Ghostty") {
        Ok(TerminalType::Ghostty)
    } else if app_exists_macos("iTerm") {
        Ok(TerminalType::ITerm)
    } else if app_exists_macos("Terminal") {
        Ok(TerminalType::TerminalApp)
    } else {
        Err(TerminalError::NoTerminalFound)
    }
}
```

**Why**: Add Ghostty detection and prioritize it as a modern terminal option

---

### Step 4: Add spawn commands for new terminal types

**File**: `src/terminal/operations.rs`
**Lines**: 31-60
**Action**: UPDATE

**Current code:**
```rust
// Line 31-60 (match statement)
match config.terminal_type {
    TerminalType::ITerm => Ok(vec![...]),
    TerminalType::TerminalApp => Ok(vec![...]),
}
```

**Required change:**
```rust
match config.terminal_type {
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
    TerminalType::Ghostty => Ok(vec![
        "osascript".to_string(),
        "-e".to_string(),
        format!(
            r#"tell application "Ghostty"
                    activate
                    delay 0.5
                end tell
                tell application "System Events"
                    keystroke "{}"
                    keystroke return
                end tell"#,
            applescript_escape(&cd_command)
        ),
    ]),
    TerminalType::Native => {
        // Use system default (detect and delegate)
        let detected = detect_terminal()?;
        let native_config = SpawnConfig::new(detected, config.working_directory.clone(), config.command.clone());
        build_spawn_command(&native_config)
    }
}
```

**Why**: Implement Ghostty AppleScript automation and native terminal delegation

---

### Step 5: Improve terminal selection in handler

**File**: `src/terminal/handler.rs`
**Lines**: 19-25
**Action**: UPDATE

**Current code:**
```rust
// Line 19-25
let terminal_type = if let Some(preferred) = &config.terminal.preferred {
    match preferred.as_str() {
        "iterm2" | "iterm" => TerminalType::ITerm,
        "terminal" => TerminalType::TerminalApp,
        _ => operations::detect_terminal()?,
    }
} else {
    operations::detect_terminal()?
};
```

**Required change:**
```rust
let terminal_type = if let Some(preferred) = &config.terminal.preferred {
    match preferred.as_str() {
        "iterm2" | "iterm" => TerminalType::ITerm,
        "terminal" => TerminalType::TerminalApp,
        "ghostty" => TerminalType::Ghostty,
        "native" => TerminalType::Native,
        _ => {
            warn!(
                event = "terminal.unknown_preference",
                preferred = preferred,
                message = "Unknown terminal preference, falling back to detection"
            );
            operations::detect_terminal()?
        }
    }
} else {
    operations::detect_terminal()?
};
```

**Why**: Support all expected terminal types and provide better error handling

---

### Step 6: Add tests for new terminal types

**File**: `src/terminal/types.rs`
**Lines**: 70-90
**Action**: UPDATE

**Test cases to add:**
```rust
#[test]
fn test_terminal_type_display_extended() {
    assert_eq!(TerminalType::ITerm.to_string(), "iterm");
    assert_eq!(TerminalType::TerminalApp.to_string(), "terminal");
    assert_eq!(TerminalType::Ghostty.to_string(), "ghostty");
    assert_eq!(TerminalType::Native.to_string(), "native");
}

#[test]
fn test_spawn_config_ghostty() {
    let config = SpawnConfig::new(
        TerminalType::Ghostty,
        PathBuf::from("/tmp/test"),
        "kiro-cli chat".to_string(),
    );
    
    assert_eq!(config.terminal_type, TerminalType::Ghostty);
}
```

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/terminal/operations.rs:31-50
// Pattern for terminal-specific AppleScript commands
match config.terminal_type {
    TerminalType::ITerm => Ok(vec![
        "osascript".to_string(),
        "-e".to_string(),
        format!(r#"tell application "iTerm"..."#, ...),
    ]),
}
```

```rust
// SOURCE: src/terminal/handler.rs:22-24
// Pattern for config preference mapping
match preferred.as_str() {
    "iterm2" | "iterm" => TerminalType::ITerm,
    "terminal" => TerminalType::TerminalApp,
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Ghostty not installed | Fall back to detection, warn user |
| Native detection fails | Use iTerm as final fallback |
| AppleScript permissions | Provide clear error message with instructions |
| Cross-platform support | Start with macOS, add Linux/Windows in future |

---

## Validation

### Automated Checks

```bash
cargo test terminal::types
cargo test terminal::operations
cargo check
```

### Manual Verification

1. Run regression test: `./scripts/regression-test.sh`
2. Test each terminal type: `shards create test-ghostty --agent kiro --terminal ghostty`
3. Verify config preferences work: Set `preferred = "ghostty"` in config
4. Test fallback behavior when terminal not available

---

## Scope Boundaries

**IN SCOPE:**
- Add Ghostty and Native terminal types
- Improve terminal selection logic
- Fix CLI terminal override
- Update tests for new types

**OUT OF SCOPE (do not touch):**
- Cross-platform terminal support (Linux/Windows)
- Advanced terminal configuration options
- Terminal-specific feature detection
- GUI terminal selection interface

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-20T15:14:44.177+02:00
- **Artifact**: `.archon/artifacts/issues/issue-7.md`
