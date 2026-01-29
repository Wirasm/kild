# Feature: Add --wait flag to kild-peek

## Summary

Add `--wait` and `--timeout` flags to `kild-peek screenshot` and `kild-peek assert` commands that poll for a window to appear before taking action. This enables reliable testing of app startup scenarios where a window may not exist immediately when the command is executed.

## User Story

As a developer testing native app startup
I want to wait for a window to appear before screenshotting
So that I can reliably capture startup states without race conditions

## Problem Statement

When launching an app and immediately trying to screenshot or assert on it, the window may not exist yet. Currently, the command fails immediately with "Window not found" error. Users must add manual delays (`sleep`) which are unreliable (too short = failure, too long = slow tests).

## Solution Statement

Add polling logic that repeatedly checks for the target window at configurable intervals until it appears or a timeout is reached. The `--wait` flag enables polling, and `--timeout` sets the maximum wait time in milliseconds (default: 30000ms = 30 seconds). Polling interval is fixed at 100ms for simplicity.

## Metadata

| Field            | Value                                                         |
| ---------------- | ------------------------------------------------------------- |
| Type             | ENHANCEMENT                                                   |
| Complexity       | MEDIUM                                                        |
| Systems Affected | kild-peek CLI, kild-peek-core window module                   |
| Dependencies     | None (uses std::thread::sleep, std::time already in project)  |
| Estimated Tasks  | 8                                                             |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Launch    │ ──────► │  screenshot │ ──────► │    FAIL     │            ║
║   │    App      │         │   --window  │         │  not found  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. cargo run -p kild-ui &                                                  ║
║   2. kild-peek screenshot --window "KILD" -o startup.png                     ║
║   3. ERROR: Window 'KILD' not found                                          ║
║                                                                               ║
║   PAIN_POINT: App hasn't started yet, need to add manual `sleep` which       ║
║   is unreliable - sleep too short = failure, sleep too long = slow tests     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Launch    │ ──────► │  screenshot │         │   SUCCESS   │            ║
║   │    App      │         │   --wait    │         │   captured  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                   │                      ▲                    ║
║                                   ▼                      │                    ║
║                          ┌─────────────┐                 │                    ║
║                          │    POLL     │─────────────────┘                    ║
║                          │  100ms loop │ ◄── waits until window appears       ║
║                          └─────────────┘     or timeout reached               ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. cargo run -p kild-ui &                                                  ║
║   2. kild-peek screenshot --window "KILD" --wait --timeout 5000 -o out.png   ║
║   3. SUCCESS: Window appeared after 1.2s, screenshot saved                   ║
║                                                                               ║
║   VALUE_ADD: Reliable startup testing without manual sleep guessing          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location       | Before                    | After                              | User Impact                    |
|----------------|---------------------------|------------------------------------|---------------------------------|
| `screenshot`   | Immediate window lookup   | Optional polling with `--wait`     | Reliable startup testing       |
| `assert`       | Immediate window lookup   | Optional polling with `--wait`     | Reliable existence assertions  |
| Timeout        | N/A                       | `--timeout` flag (default 30000ms) | Configurable wait duration     |
| Error messages | "Window not found"        | "Window not found after Xms"       | Clear timeout indication       |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-peek/src/app.rs` | 54-111 | CLI argument structure for screenshot command - MIRROR this pattern |
| P0 | `crates/kild-peek/src/commands.rs` | 146-208 | Screenshot command handler - ADD wait logic here |
| P0 | `crates/kild-peek-core/src/window/handler.rs` | 261-318 | `find_window_by_title` - WRAP with polling |
| P1 | `crates/kild-peek-core/src/window/errors.rs` | 1-45 | WindowError types - ADD timeout variant |
| P1 | `crates/kild-core/src/process/pid_file.rs` | 44-96 | MIRROR this retry/polling pattern |
| P2 | `crates/kild-peek/src/commands.rs` | 286-347 | Assert command handler - ADD wait logic here |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [std::time::Instant](https://doc.rust-lang.org/std/time/struct.Instant.html) | elapsed() | For timeout tracking |
| [std::thread::sleep](https://doc.rust-lang.org/std/thread/fn.sleep.html) | sleep duration | For polling interval |

---

## Patterns to Mirror

**CLI_ARGUMENT_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/app.rs:91-96
// COPY THIS PATTERN for boolean flags:
.arg(
    Arg::new("base64")
        .long("base64")
        .help("Output base64 encoded image (default if no --output)")
        .action(ArgAction::SetTrue),
)
```

**CLI_VALUE_ARGUMENT_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/app.rs:78-84
// COPY THIS PATTERN for numeric value with default:
.arg(
    Arg::new("monitor")
        .long("monitor")
        .short('m')
        .help("Capture specific monitor by index (default: primary)")
        .value_parser(clap::value_parser!(usize)),
)
```

**RETRY_LOOP_PATTERN:**
```rust
// SOURCE: crates/kild-core/src/process/pid_file.rs:44-94
// COPY THIS PATTERN for polling:
pub fn read_pid_file_with_retry(
    pid_file: &Path,
    max_attempts: u32,
    initial_delay_ms: u64,
) -> Result<Option<u32>, ProcessError> {
    let mut delay = Duration::from_millis(initial_delay_ms);

    for attempt in 1..=max_attempts {
        debug!(
            event = "core.pid_file.read_attempt",
            attempt,
            path = %pid_file.display()
        );
        // ... attempt operation ...
        std::thread::sleep(delay);
    }
}
```

**WINDOW_FIND_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:261-318
// This is the function to wrap with polling:
pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_started", title = title);
    // ... matching logic ...
    Err(WindowError::WindowNotFound {
        title: title.to_string(),
    })
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:7-8
// COPY THIS PATTERN for new events:
use tracing::{debug, info, warn};

info!(event = "core.window.find_started", title = title);
info!(event = "core.window.find_completed", title = original_title, match_type = match_type.as_str());
```

**ERROR_DEFINITION_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/errors.rs:3-22
// COPY THIS PATTERN for new error variant:
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },
    // ... add new variant here
}
```

**CLI_MATCH_EXTRACTION_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:147-157
// COPY THIS PATTERN for extracting args:
let window_title = matches.get_one::<String>("window");
let base64_flag = matches.get_flag("base64");
let quality = *matches.get_one::<u8>("quality").unwrap_or(&85);
```

---

## Files to Change

| File                                              | Action | Justification                                   |
|---------------------------------------------------|--------|-------------------------------------------------|
| `crates/kild-peek-core/src/window/errors.rs`      | UPDATE | Add `WaitTimeout` error variant                 |
| `crates/kild-peek-core/src/window/handler.rs`     | UPDATE | Add polling wrapper functions                   |
| `crates/kild-peek-core/src/window/mod.rs`         | UPDATE | Re-export new polling functions                 |
| `crates/kild-peek/src/app.rs`                     | UPDATE | Add --wait and --timeout CLI args               |
| `crates/kild-peek/src/commands.rs`                | UPDATE | Integrate wait logic in handlers                |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No configurable poll interval**: Fixed at 100ms for simplicity. Configurable interval adds complexity with minimal benefit.
- **No exponential backoff**: Simple fixed-interval polling is sufficient for this use case. Windows either exist or don't.
- **No --wait for list command**: List always returns immediately - no window to wait for.
- **No --wait for diff command**: Diff compares files, not windows - no waiting needed.
- **No --wait for monitor targets**: Monitors always exist. Only window targets need waiting.
- **No progress indicator**: Silent waiting is fine for scripting. Progress would complicate output parsing.

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `crates/kild-peek-core/src/window/errors.rs`

- **ACTION**: ADD new error variant for wait timeout
- **IMPLEMENT**:
  ```rust
  #[error("Window '{title}' not found after {timeout_ms}ms")]
  WaitTimeout { title: String, timeout_ms: u64 },
  ```
- **MIRROR**: `crates/kild-peek-core/src/window/errors.rs:8-9` - follow `WindowNotFound` pattern
- **IMPORTS**: None needed
- **ALSO UPDATE**: `error_code()` match to return `"WINDOW_WAIT_TIMEOUT"`, `is_user_error()` to include this variant
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 2: UPDATE `crates/kild-peek-core/src/window/handler.rs` - Add poll function

- **ACTION**: ADD new polling wrapper function
- **IMPLEMENT**:
  ```rust
  use std::time::{Duration, Instant};

  /// Find a window by title, polling until found or timeout
  pub fn find_window_by_title_with_wait(
      title: &str,
      timeout_ms: u64,
  ) -> Result<WindowInfo, WindowError> {
      info!(event = "core.window.poll_started", title = title, timeout_ms = timeout_ms);

      let start = Instant::now();
      let timeout = Duration::from_millis(timeout_ms);
      let poll_interval = Duration::from_millis(100);
      let mut attempt = 0u32;

      loop {
          attempt += 1;
          match find_window_by_title(title) {
              Ok(window) => {
                  info!(
                      event = "core.window.poll_completed",
                      title = title,
                      attempts = attempt,
                      elapsed_ms = start.elapsed().as_millis()
                  );
                  return Ok(window);
              }
              Err(WindowError::WindowNotFound { .. }) => {
                  if start.elapsed() >= timeout {
                      warn!(
                          event = "core.window.poll_timeout",
                          title = title,
                          timeout_ms = timeout_ms,
                          attempts = attempt
                      );
                      return Err(WindowError::WaitTimeout {
                          title: title.to_string(),
                          timeout_ms,
                      });
                  }
                  debug!(
                      event = "core.window.poll_attempt",
                      title = title,
                      attempt = attempt,
                      elapsed_ms = start.elapsed().as_millis()
                  );
                  std::thread::sleep(poll_interval);
              }
              Err(e) => return Err(e), // Propagate non-NotFound errors immediately
          }
      }
  }
  ```
- **MIRROR**: `crates/kild-core/src/process/pid_file.rs:44-94` for loop/sleep pattern
- **IMPORTS**: Add `use std::time::{Duration, Instant};` at top
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 3: UPDATE `crates/kild-peek-core/src/window/handler.rs` - Add app poll functions

- **ACTION**: ADD polling wrappers for `find_window_by_app` and `find_window_by_app_and_title`
- **IMPLEMENT**:
  ```rust
  /// Find a window by app name, polling until found or timeout
  pub fn find_window_by_app_with_wait(
      app: &str,
      timeout_ms: u64,
  ) -> Result<WindowInfo, WindowError> {
      info!(event = "core.window.poll_by_app_started", app = app, timeout_ms = timeout_ms);

      let start = Instant::now();
      let timeout = Duration::from_millis(timeout_ms);
      let poll_interval = Duration::from_millis(100);
      let mut attempt = 0u32;

      loop {
          attempt += 1;
          match find_window_by_app(app) {
              Ok(window) => {
                  info!(
                      event = "core.window.poll_by_app_completed",
                      app = app,
                      attempts = attempt,
                      elapsed_ms = start.elapsed().as_millis()
                  );
                  return Ok(window);
              }
              Err(WindowError::WindowNotFoundByApp { .. }) => {
                  if start.elapsed() >= timeout {
                      warn!(
                          event = "core.window.poll_by_app_timeout",
                          app = app,
                          timeout_ms = timeout_ms,
                          attempts = attempt
                      );
                      return Err(WindowError::WaitTimeout {
                          title: format!("app:{}", app),
                          timeout_ms,
                      });
                  }
                  debug!(
                      event = "core.window.poll_by_app_attempt",
                      app = app,
                      attempt = attempt,
                      elapsed_ms = start.elapsed().as_millis()
                  );
                  std::thread::sleep(poll_interval);
              }
              Err(e) => return Err(e),
          }
      }
  }

  /// Find a window by app and title, polling until found or timeout
  pub fn find_window_by_app_and_title_with_wait(
      app: &str,
      title: &str,
      timeout_ms: u64,
  ) -> Result<WindowInfo, WindowError> {
      info!(
          event = "core.window.poll_by_app_and_title_started",
          app = app,
          title = title,
          timeout_ms = timeout_ms
      );

      let start = Instant::now();
      let timeout = Duration::from_millis(timeout_ms);
      let poll_interval = Duration::from_millis(100);
      let mut attempt = 0u32;

      loop {
          attempt += 1;
          match find_window_by_app_and_title(app, title) {
              Ok(window) => {
                  info!(
                      event = "core.window.poll_by_app_and_title_completed",
                      app = app,
                      title = title,
                      attempts = attempt,
                      elapsed_ms = start.elapsed().as_millis()
                  );
                  return Ok(window);
              }
              Err(WindowError::WindowNotFound { .. } | WindowError::WindowNotFoundByApp { .. }) => {
                  if start.elapsed() >= timeout {
                      warn!(
                          event = "core.window.poll_by_app_and_title_timeout",
                          app = app,
                          title = title,
                          timeout_ms = timeout_ms,
                          attempts = attempt
                      );
                      return Err(WindowError::WaitTimeout {
                          title: format!("{}:{}", app, title),
                          timeout_ms,
                      });
                  }
                  debug!(
                      event = "core.window.poll_by_app_and_title_attempt",
                      app = app,
                      title = title,
                      attempt = attempt,
                      elapsed_ms = start.elapsed().as_millis()
                  );
                  std::thread::sleep(poll_interval);
              }
              Err(e) => return Err(e),
          }
      }
  }
  ```
- **MIRROR**: Task 2 pattern
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 4: UPDATE `crates/kild-peek-core/src/window/mod.rs`

- **ACTION**: Re-export new polling functions
- **IMPLEMENT**: Add to existing pub use statement:
  ```rust
  pub use handler::{
      // existing exports...
      find_window_by_title_with_wait,
      find_window_by_app_with_wait,
      find_window_by_app_and_title_with_wait,
  };
  ```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 5: UPDATE `crates/kild-peek/src/app.rs` - Add screenshot args

- **ACTION**: ADD `--wait` and `--timeout` arguments to screenshot command
- **IMPLEMENT**: After line 111 (after quality arg), add:
  ```rust
  .arg(
      Arg::new("wait")
          .long("wait")
          .help("Wait for window to appear (polls until found or timeout)")
          .action(ArgAction::SetTrue),
  )
  .arg(
      Arg::new("timeout")
          .long("timeout")
          .help("Timeout in milliseconds when using --wait (default: 30000)")
          .value_parser(clap::value_parser!(u64))
          .default_value("30000"),
  )
  ```
- **MIRROR**: `crates/kild-peek/src/app.rs:91-96` for flag pattern
- **VALIDATE**: `cargo build -p kild-peek`

### Task 6: UPDATE `crates/kild-peek/src/app.rs` - Add assert args

- **ACTION**: ADD `--wait` and `--timeout` arguments to assert command
- **IMPLEMENT**: After line 192 (after json arg), add same args as Task 5
- **VALIDATE**: `cargo build -p kild-peek`

### Task 7: UPDATE `crates/kild-peek/src/commands.rs` - Screenshot wait logic

- **ACTION**: Modify screenshot handler to use wait functions when --wait flag is set
- **IMPLEMENT**:
  1. Add import: `use kild_peek_core::window::{find_window_by_title_with_wait, find_window_by_app_with_wait, find_window_by_app_and_title_with_wait};`
  2. Extract wait args after line 157:
     ```rust
     let wait_flag = matches.get_flag("wait");
     let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
     ```
  3. Create new function `build_capture_request_with_wait` that uses wait functions when `wait_flag` is true
  4. Use the appropriate function based on wait_flag before calling `capture()`
- **MIRROR**: `crates/kild-peek/src/commands.rs:147-157` for arg extraction
- **GOTCHA**: Only window targets support wait - monitor targets should ignore wait flag
- **VALIDATE**: `cargo build -p kild-peek && cargo test -p kild-peek`

### Task 8: UPDATE `crates/kild-peek/src/commands.rs` - Assert wait logic

- **ACTION**: Modify assert handler to support waiting for window before asserting
- **IMPLEMENT**:
  1. Extract wait args after line 293:
     ```rust
     let wait_flag = matches.get_flag("wait");
     let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
     ```
  2. Modify `resolve_window_title` calls to use wait variants when `wait_flag` is true
  3. Create helper `resolve_window_title_with_wait` that uses the wait functions
- **MIRROR**: Task 7 pattern
- **VALIDATE**: `cargo build -p kild-peek && cargo test -p kild-peek`

---

## Testing Strategy

### Unit Tests to Write

| Test File                                           | Test Cases                                      | Validates                |
|----------------------------------------------------|-------------------------------------------------|--------------------------|
| `crates/kild-peek-core/src/window/handler.rs`      | poll_timeout_returns_error, poll_immediate_find | Polling logic            |
| `crates/kild-peek/src/app.rs`                      | test_cli_screenshot_wait_flag, test_cli_assert_wait_flag | CLI parsing     |

### Edge Cases Checklist

- [ ] Window exists immediately - should return without waiting
- [ ] Window never appears - should timeout with clear error
- [ ] Window appears mid-poll - should return once found
- [ ] Non-window errors (enumeration failed) - should propagate immediately, not retry
- [ ] Monitor targets with --wait - should be ignored (monitors always exist)
- [ ] --wait without --timeout - should use default 30000ms
- [ ] --timeout without --wait - timeout value should be ignored

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p kild-peek-core && cargo test -p kild-peek
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: MANUAL_VALIDATION

1. Test immediate find (window exists):
   ```bash
   # With a terminal window open
   cargo run -p kild-peek -- screenshot --window "Terminal" --wait -o /tmp/term.png
   # Should succeed immediately
   ```

2. Test wait then find:
   ```bash
   # Launch app in background, immediately try to screenshot
   (sleep 2 && open -a Calculator) &
   cargo run -p kild-peek -- screenshot --window "Calculator" --wait --timeout 5000 -o /tmp/calc.png
   # Should wait ~2s then succeed
   ```

3. Test timeout:
   ```bash
   cargo run -p kild-peek -- screenshot --window "NONEXISTENT_WINDOW_XYZ" --wait --timeout 2000 -o /tmp/fail.png
   # Should fail after 2s with timeout error
   ```

4. Test assert with wait:
   ```bash
   cargo run -p kild-peek -- assert --window "Terminal" --exists --wait --timeout 1000
   # Should pass
   ```

---

## Acceptance Criteria

- [ ] `kild-peek screenshot --window X --wait` polls until window X appears
- [ ] `kild-peek assert --window X --exists --wait` polls until window X exists
- [ ] `--timeout` flag controls maximum wait time (default 30000ms)
- [ ] Timeout produces clear error: "Window 'X' not found after Yms"
- [ ] Window exists immediately: returns without unnecessary delay
- [ ] Non-window errors (permission denied, enumeration failed): propagate immediately
- [ ] All existing tests continue to pass
- [ ] Code follows existing patterns (logging, error handling, CLI structure)

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (lint + type-check) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] Level 4: Manual validation passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk                          | Likelihood | Impact | Mitigation                                              |
|-------------------------------|------------|--------|---------------------------------------------------------|
| Blocking main thread          | LOW        | MED    | Documented limitation; users can Ctrl+C to interrupt    |
| Rapid polling causes CPU load | LOW        | LOW    | 100ms interval is conservative enough                   |
| CI test flakiness             | MED        | LOW    | Tests use nonexistent windows, deterministic timeouts   |

---

## Notes

- Poll interval is hardcoded at 100ms. This balances responsiveness vs CPU usage.
- The `--wait` flag only affects window-based targets. Monitor targets always exist immediately.
- The implementation mirrors the existing `read_pid_file_with_retry` pattern from kild-core for consistency.
- Future enhancement: could add `--poll-interval` flag if users need faster/slower polling, but YAGNI for now.
