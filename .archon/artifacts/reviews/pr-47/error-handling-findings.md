# Error Handling Findings: PR #47

**Reviewer**: error-handling-agent
**Date**: 2026-01-21T14:32:00Z
**Error Handlers Reviewed**: 4

---

## Summary

This PR implements terminal window closing during session destroy with deliberately non-fatal error handling. The design is intentional - terminal close failures should not block session destruction. However, there are several error handling patterns that warrant attention: silent discarding of results with `let _ =`, overly broad error suppression, and string-based error matching that could miss edge cases.

**Verdict**: NEEDS_DISCUSSION

---

## Findings

### Finding 1: Silent Result Discarding in destroy_session

**Severity**: MEDIUM
**Category**: silent-failure
**Location**: `src/sessions/handler.rs:188`

**Issue**:
The result of `close_terminal()` is silently discarded with `let _ =`. While this is intentional (terminal close is best-effort), the pattern makes it impossible to distinguish between:
- Close succeeded
- Close failed but was recoverable
- Close failed due to unexpected error

**Evidence**:
```rust
// Current error handling at src/sessions/handler.rs:188
if let Some(ref terminal_type) = session.terminal_type {
    info!(event = "session.destroy_close_terminal", terminal_type = %terminal_type);
    // Best-effort - don't fail destroy if terminal close fails
    let _ = terminal::handler::close_terminal(terminal_type);
}
```

**Hidden Errors**:
This pattern could silently hide:
- AppleScript execution failures (osascript not found, permissions)
- Terminal detection failures when using Native type
- IO errors during script execution
- Any future error types added to TerminalError

**User Impact**:
User has no feedback if terminal close fails. They may see orphaned terminal windows without understanding why, leading to confusion about whether `shards destroy` is working correctly.

---

#### Fix Suggestions

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A | Log the result explicitly at call site | Better visibility, matches info! already present | Redundant with handler logging |
| B | Keep current pattern (already logged in handler) | Simple, handler already logs | Call site looks like error is ignored |
| C | Add structured result type for partial success | Rich feedback to user | Over-engineering for best-effort operation |

**Recommended**: Option B (keep current)

**Reasoning**:
Looking at `src/terminal/handler.rs:183-199`, the `close_terminal` function already:
1. Logs at INFO level on success
2. Logs at WARN level on failure with error details
3. Always returns `Ok(())` by design

The `let _ =` pattern is intentional here. The handler's internal logging provides sufficient observability. Adding more logging at the call site would be redundant.

**Codebase Pattern Reference**:
```rust
// SOURCE: src/terminal/handler.rs:183-199
pub fn close_terminal(terminal_type: &TerminalType) -> Result<(), TerminalError> {
    info!(event = "terminal.close_started", terminal_type = %terminal_type);

    let result = operations::close_terminal_window(terminal_type);

    match &result {
        Ok(()) => info!(event = "terminal.close_completed", terminal_type = %terminal_type),
        Err(e) => warn!(
            event = "terminal.close_failed",
            terminal_type = %terminal_type,
            error = %e,
            message = "Continuing with destroy despite terminal close failure"
        ),
    }

    // Always return Ok - terminal close failure should not block destroy
    Ok(())
}
```

**Status**: ACCEPTABLE - Pattern is intentional and handler provides logging. Consider adding a comment at the call site explaining this for future maintainers.

---

### Finding 2: String-Based Error Matching for Window Close Detection

**Severity**: MEDIUM
**Category**: unsafe-fallback
**Location**: `src/terminal/operations.rs:203`

**Issue**:
Error detection relies on substring matching in stderr output. This approach is fragile:
- Different AppleScript versions may produce different error messages
- Localized systems may have translated error messages
- The strings "window" and "count" are very generic and could match unrelated errors

**Evidence**:
```rust
// Current error handling at src/terminal/operations.rs:200-209
if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Don't fail if window was already closed - this is expected behavior
    if stderr.contains("window") || stderr.contains("count") {
        debug!(
            event = "terminal.close_window_not_found",
            terminal_type = %terminal_type,
            message = "Window may have been closed manually"
        );
        return Ok(());
    }
    // ...
}
```

**Hidden Errors**:
This pattern could incorrectly treat as "window not found":
- "window process crashed" errors
- "count overflow" errors
- Any error message containing these common English words
- Legitimate failures that happen to contain these words

**User Impact**:
Legitimate errors could be misclassified as "window already closed", making debugging difficult when terminal close actually fails for other reasons.

---

#### Fix Suggestions

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A | Match more specific error patterns | Reduces false positives | May miss variations |
| B | Log the full stderr at DEBUG for all failures | Better debugging | More log noise |
| C | Treat ALL AppleScript failures as non-fatal | Simpler, already non-fatal anyway | Less precision in logs |

**Recommended**: Option C

**Reasoning**:
Since the function already returns `Ok(())` for ALL failure cases (lines 216-217), the string matching only affects which log message is emitted. The string matching adds complexity without changing behavior. Either:
1. All AppleScript failures should be treated uniformly (current behavior), or
2. We need much more specific pattern matching to be meaningful

The current middle ground provides false confidence in the detection logic.

**Recommended Fix**:
```rust
// Simplified error handling - all failures are non-fatal
if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    debug!(
        event = "terminal.close_failed",
        terminal_type = %terminal_type,
        stderr = %stderr,
        message = "Terminal close did not succeed - window may have been closed manually"
    );
    // Non-fatal - don't block destroy on terminal close failure
    return Ok(());
}
```

**Codebase Pattern Reference**:
```rust
// SOURCE: src/terminal/operations.rs:138-155
// Pattern for checking terminal app existence - uses boolean result, not string matching
fn app_exists_macos(app_name: &str) -> bool {
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(r#"tell application "System Events" to exists..."#, app_name))
        .output()
        .map(|output| {
            output.status.success() &&
            String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
        .unwrap_or(false)
}
```

---

### Finding 3: Recursive Error Propagation for Native Terminal Type

**Severity**: LOW
**Category**: missing-logging
**Location**: `src/terminal/operations.rs:183-186`

**Issue**:
When `TerminalType::Native` is used, the function recursively calls itself after detecting the actual terminal. If detection fails, the error propagates but there's no indication that it originated from a Native type resolution.

**Evidence**:
```rust
// Current error handling at src/terminal/operations.rs:183-186
TerminalType::Native => {
    // For Native, try to detect what terminal is running
    let detected = detect_terminal()?;
    return close_terminal_window(&detected);
}
```

**Hidden Errors**:
This pattern could obscure the origin of:
- `NoTerminalFound` errors - user won't know it was because Native couldn't resolve
- Any detection errors - context about why detection was needed is lost

**User Impact**:
If a user explicitly specifies Native terminal and detection fails, the error message won't indicate that Native resolution failed - it will just say "no terminal found".

---

#### Fix Suggestions

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A | Add debug log before recursion | Shows resolution path | More log output |
| B | Wrap error with context | Clear error chain | Error type changes |
| C | Keep as-is (detection already logs) | Simple | Less explicit |

**Recommended**: Option A

**Reasoning**:
Adding a single debug log before the recursive call would document the resolution path without changing error types or adding complexity.

**Recommended Fix**:
```rust
TerminalType::Native => {
    debug!(event = "terminal.close_native_resolving", message = "Detecting terminal for Native type");
    let detected = detect_terminal()?;
    return close_terminal_window(&detected);
}
```

**Codebase Pattern Reference**:
```rust
// SOURCE: src/terminal/operations.rs:110-118
// Pattern in build_spawn_command handles Native the same way
TerminalType::Native => {
    // Use system default (detect and delegate)
    let detected = detect_terminal()?;
    if detected == TerminalType::Native {
        return Err(TerminalError::NoTerminalFound);
    }
    let native_config = SpawnConfig::new(detected, ...);
    build_spawn_command(&native_config)
}
```

---

### Finding 4: TerminalCloseFailed Error Variant Never Used

**Severity**: LOW
**Category**: broad-catch
**Location**: `src/terminal/errors.rs:26-27`

**Issue**:
The PR adds a new error variant `TerminalCloseFailed` but it's never actually used in the code. The close functions return `Ok(())` on all paths. This is dead code that suggests the error handling design changed during implementation.

**Evidence**:
```rust
// Added to src/terminal/errors.rs:26-27
#[error("Failed to close terminal window for {terminal}: {message}")]
TerminalCloseFailed { terminal: String, message: String },
```

**Hidden Errors**:
Not directly hiding errors, but the unused variant:
- Misleads future developers about error possibilities
- Increases maintenance burden
- Suggests incomplete implementation

**User Impact**:
None directly - this is code hygiene.

---

#### Fix Suggestions

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A | Remove unused error variant | Cleaner code | May need it later |
| B | Keep for future use | Ready when needed | Dead code |
| C | Use it when AppleScript execution fails | More accurate errors | Changes non-fatal design |

**Recommended**: Option A

**Reasoning**:
YAGNI principle. If terminal close becomes fatal in the future, the error variant can be added then. Currently it adds confusion about what errors are actually possible.

**Recommended Fix**:
Remove from `src/terminal/errors.rs`:
```rust
// DELETE these lines
#[error("Failed to close terminal window for {terminal}: {message}")]
TerminalCloseFailed { terminal: String, message: String },

// DELETE from error_code match
TerminalError::TerminalCloseFailed { .. } => "TERMINAL_CLOSE_FAILED",
```

---

## Error Handler Audit

| Location | Type | Logging | User Feedback | Specificity | Verdict |
|----------|------|---------|---------------|-------------|---------|
| `sessions/handler.rs:188` | let _ discard | GOOD (in handler) | N/A (non-fatal) | N/A | PASS |
| `terminal/handler.rs:186-199` | match result | GOOD | N/A (non-fatal) | GOOD | PASS |
| `terminal/operations.rs:196-198` | map_err | GOOD | N/A | GOOD | PASS |
| `terminal/operations.rs:200-217` | if !success | BAD (string match) | N/A | BAD | NEEDS WORK |

---

## Statistics

| Severity | Count | Auto-fixable |
|----------|-------|--------------|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 2 | 1 |
| LOW | 2 | 2 |

---

## Silent Failure Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Terminal window stays open after destroy | MEDIUM | LOW (UX inconvenience) | Already mitigated by design |
| AppleScript permissions error masked | LOW | LOW (logged at WARN) | Handler logging sufficient |
| String matching false positive | LOW | LOW (logs affected, not behavior) | Simplify to uniform handling |
| Native terminal detection fails silently | LOW | MEDIUM (confusing error) | Add debug log |

---

## Patterns Referenced

| File | Lines | Pattern |
|------|-------|---------|
| `src/terminal/handler.rs` | 183-199 | Best-effort operation with internal logging |
| `src/terminal/operations.rs` | 138-155 | Boolean result from osascript |
| `src/sessions/handler.rs` | 195-220 | Fatal vs non-fatal error handling in destroy |
| `src/terminal/operations.rs` | 110-118 | Native terminal type resolution |

---

## Positive Observations

1. **Intentional non-fatal design**: The decision to make terminal close non-fatal is well-documented in comments and correctly implemented. This follows the principle that destroy should be robust.

2. **Comprehensive logging**: The handler layer (`close_terminal`) provides good observability with INFO/WARN level logs that capture both success and failure paths.

3. **Platform-aware implementation**: The `#[cfg(not(target_os = "macos"))]` variant returns `Ok(())` immediately, correctly handling unsupported platforms without error.

4. **Graceful degradation**: When terminal_type is None (old sessions), the code simply skips the close step rather than failing.

5. **Consistent with codebase patterns**: The `let _ =` pattern for intentionally ignored results is used elsewhere in the codebase (e.g., `src/git/handler.rs:461`, test cleanup).

---

## Metadata

- **Agent**: error-handling-agent
- **Timestamp**: 2026-01-21T14:32:00Z
- **Artifact**: `.archon/artifacts/reviews/pr-47/error-handling-findings.md`
