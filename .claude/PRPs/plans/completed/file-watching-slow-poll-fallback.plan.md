# Feature: File Watching with Slow Poll Fallback

## Summary

Replace the current 5-second polling mechanism in kild-ui with a hybrid approach: file system watching (via `notify` crate) for instant updates when CLI modifies session files, combined with a 60-second slow poll fallback for edge cases like direct process termination. This improves responsiveness (~100ms latency vs 5s) while reducing unnecessary CPU wake-ups.

## User Story

As a developer using KILD
I want instant UI updates when I create, stop, or destroy kilds via CLI
So that I can see my changes reflected immediately without waiting up to 5 seconds

## Problem Statement

The current 5-second polling interval creates three issues:
1. **Delayed feedback**: Up to 5s delay after CLI operations (create, stop, destroy, open)
2. **Unnecessary work**: CPU wakes up every 5s even when nothing changed
3. **Resource waste**: Continuous polling when sessions are stable

## Solution Statement

Implement a hybrid event system:
1. **File watcher** (notify crate) watches `~/.kild/sessions/` for file changes → triggers immediate refresh
2. **Slow poll** (60s) as fallback for edge cases (process crashes, external changes, missed events)
3. **Graceful degradation**: If file watching fails, fall back to current 5s polling with warning log

| Trigger | What it catches | Latency |
|---------|-----------------|---------|
| File watcher | CLI actions: create, destroy, stop, open | ~100ms |
| Poll (60s) | Process crashes, external changes, missed events | ≤60s |

## Metadata

| Field            | Value |
| ---------------- | ----- |
| Type             | ENHANCEMENT |
| Complexity       | MEDIUM |
| Systems Affected | kild-ui |
| Dependencies     | notify = "8.0" (new workspace dependency) |
| Estimated Tasks  | 6 |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │  Terminal   │ ──────► │ kild create │ ──────► │  Session    │            ║
║   │   (CLI)     │         │   feature   │         │  file.json  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                          │                    ║
║                                                          │                    ║
║                                                   0-5s delay                  ║
║                                                          │                    ║
║                                                          ▼                    ║
║                                                   ┌─────────────┐            ║
║                                                   │   kild-ui   │            ║
║                                                   │  (polling)  │            ║
║                                                   └─────────────┘            ║
║                                                                               ║
║   USER_FLOW: Run CLI command → Wait 0-5 seconds → See update in UI           ║
║   PAIN_POINT: Up to 5 second delay feels unresponsive                         ║
║   DATA_FLOW: CLI writes file → Poll timer expires → UI reads directory        ║
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
║   │  Terminal   │ ──────► │ kild create │ ──────► │  Session    │            ║
║   │   (CLI)     │         │   feature   │         │  file.json  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                          │                    ║
║                                               ┌──────────┴──────────┐        ║
║                                               │                     │        ║
║                                               ▼                     ▼        ║
║                                        ┌───────────┐         ┌───────────┐   ║
║                                        │  File     │         │  Slow     │   ║
║                                        │  Watcher  │         │  Poll 60s │   ║
║                                        │  ~100ms   │         │  (backup) │   ║
║                                        └─────┬─────┘         └─────┬─────┘   ║
║                                              │                     │         ║
║                                              └──────────┬──────────┘         ║
║                                                         ▼                    ║
║                                                  ┌─────────────┐             ║
║                                                  │   kild-ui   │             ║
║                                                  │  (instant)  │             ║
║                                                  └─────────────┘             ║
║                                                                               ║
║   USER_FLOW: Run CLI command → See update in UI (~100ms)                      ║
║   VALUE_ADD: Instant feedback, lower CPU usage, 60s backup catches edge cases ║
║   DATA_FLOW: CLI writes file → FSEvents notifies → UI refreshes immediately   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| CLI `kild create` | 0-5s delay | ~100ms delay | Instant visual confirmation |
| CLI `kild stop` | 0-5s delay | ~100ms delay | Immediate status update |
| CLI `kild destroy` | 0-5s delay | ~100ms delay | Kild removed from list instantly |
| Process crash (kill -9) | 0-5s delay | 0-60s delay | Acceptable for rare edge case |
| CPU usage | Wake every 5s | Wake on events + every 60s | Reduced CPU usage |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/views/main_view.rs` | 125-162 | Task spawning pattern with `cx.spawn()` - MIRROR exactly |
| P0 | `crates/kild-ui/src/state.rs` | 371-406 | `update_statuses_only()` and `refresh_sessions()` - understand refresh logic |
| P0 | `crates/kild-ui/src/state.rs` | 546-578 | `count_session_files()` - how sessions dir is accessed |
| P1 | `crates/kild-ui/src/refresh.rs` | 1-10 | Current refresh interval constant |
| P1 | `crates/kild-core/src/config/defaults.rs` | 117-129 | `sessions_dir()` method |
| P2 | `crates/kild-ui/src/main.rs` | 1-44 | Module registration pattern |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [notify docs v8.x](https://docs.rs/notify/latest/notify/) | `recommended_watcher()` | Platform-native watcher creation |
| [notify EventKind](https://docs.rs/notify/latest/notify/event/enum.EventKind.html) | Event variants | Filter Create/Modify/Remove events |

---

## Patterns to Mirror

**TASK_SPAWNING_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/main_view.rs:134-155
// COPY THIS PATTERN:
let refresh_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
    tracing::debug!(event = "ui.auto_refresh.started");

    loop {
        cx.background_executor()
            .timer(crate::refresh::REFRESH_INTERVAL)
            .await;

        if let Err(e) = this.update(cx, |view, cx| {
            tracing::debug!(event = "ui.auto_refresh.tick");
            view.state.update_statuses_only();
            cx.notify();
        }) {
            tracing::debug!(
                event = "ui.auto_refresh.stopped",
                reason = "view_dropped",
                error = ?e
            );
            break;
        }
    }
});
```

**SESSIONS_DIR_ACCESS:**
```rust
// SOURCE: crates/kild-ui/src/state.rs:546-548
// COPY THIS PATTERN:
fn count_session_files() -> Option<usize> {
    let config = kild_core::config::Config::new();
    count_session_files_in_dir(&config.sessions_dir())
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/state.rs:379-384
// COPY THIS PATTERN:
tracing::info!(
    event = "ui.auto_refresh.session_count_mismatch",
    disk_count = count,
    memory_count = self.displays.len(),
    action = "triggering full refresh"
);
```

**GRACEFUL_DEGRADATION:**
```rust
// SOURCE: crates/kild-ui/src/state.rs:388-395
// COPY THIS PATTERN:
None => {
    // Cannot determine count - skip mismatch check and just update statuses.
    // This is a non-critical degradation (directory read failure).
    tracing::debug!(
        event = "ui.auto_refresh.count_check_skipped",
        reason = "cannot read sessions directory"
    );
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `Cargo.toml` (workspace root) | UPDATE | Add `notify = "8.0"` to workspace dependencies |
| `crates/kild-ui/Cargo.toml` | UPDATE | Add `notify.workspace = true` |
| `crates/kild-ui/src/watcher.rs` | CREATE | New module: SessionWatcher wrapping notify |
| `crates/kild-ui/src/refresh.rs` | UPDATE | Change REFRESH_INTERVAL to 60s, add DEBOUNCE_INTERVAL |
| `crates/kild-ui/src/main.rs` | UPDATE | Add `mod watcher;` |
| `crates/kild-ui/src/views/main_view.rs` | UPDATE | Integrate watcher task alongside poll task |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Process watching (kqueue/kevent)**: Platform-specific, complex, diminishing returns for rare crash detection
- **IPC event bus**: Would require CLI changes and socket management - overkill for file change detection
- **notify-debouncer crates**: Using manual debounce (simpler, fewer deps) instead of notify-debouncer-mini/full
- **Watching individual session files**: Watch directory only (NonRecursive) - simpler, covers all cases
- **Hot-reload of watch path**: Sessions dir is fixed at startup, no need to handle path changes

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` (workspace root)

- **ACTION**: ADD `notify` to workspace dependencies
- **IMPLEMENT**: Add `notify = "8.0"` in `[workspace.dependencies]` section
- **LOCATION**: After `tempfile = "3"` line (alphabetical order not required, but group logically)
- **VALIDATE**: `cargo check --workspace` - workspace resolves dependency

### Task 2: UPDATE `crates/kild-ui/Cargo.toml`

- **ACTION**: ADD notify dependency reference
- **IMPLEMENT**: Add `notify.workspace = true` to `[dependencies]` section
- **LOCATION**: After `dirs.workspace = true`
- **VALIDATE**: `cargo check -p kild-ui` - compiles with notify available

### Task 3: UPDATE `crates/kild-ui/src/refresh.rs`

- **ACTION**: UPDATE refresh constants
- **IMPLEMENT**:
  ```rust
  use std::time::Duration;

  /// Fallback poll interval - file watcher handles most updates.
  /// This catches process crashes, external changes, missed events.
  pub const POLL_INTERVAL: Duration = Duration::from_secs(60);

  /// Debounce interval for file events to avoid rapid refreshes
  /// when multiple files change at once (e.g., bulk operations).
  pub const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);

  /// Fast poll interval used when file watching is unavailable.
  /// Falls back to previous behavior if watcher fails to initialize.
  pub const FAST_POLL_INTERVAL: Duration = Duration::from_secs(5);
  ```
- **MIRROR**: Keep same file structure, just change values and add new constants
- **VALIDATE**: `cargo check -p kild-ui`

### Task 4: CREATE `crates/kild-ui/src/watcher.rs`

- **ACTION**: CREATE new file watcher module
- **IMPLEMENT**:
  ```rust
  //! File watcher for session changes.
  //!
  //! Watches the sessions directory for file system events (create, modify, remove)
  //! to trigger immediate UI refresh when CLI operations occur.

  use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
  use std::path::Path;
  use std::sync::mpsc::{self, Receiver, TryRecvError};

  /// Watches the sessions directory for changes.
  ///
  /// Uses platform-native file watching (FSEvents on macOS, inotify on Linux)
  /// for efficient event-driven updates instead of polling.
  pub struct SessionWatcher {
      /// The underlying notify watcher. Must be kept alive.
      _watcher: RecommendedWatcher,
      /// Channel receiver for file events.
      receiver: Receiver<Result<Event, notify::Error>>,
  }

  impl SessionWatcher {
      /// Create a new watcher for the given sessions directory.
      ///
      /// Returns `None` if the watcher cannot be created (e.g., platform not supported,
      /// permissions issue, or directory doesn't exist yet).
      pub fn new(sessions_dir: &Path) -> Option<Self> {
          let (tx, rx) = mpsc::channel();

          let mut watcher = match notify::recommended_watcher(tx) {
              Ok(w) => w,
              Err(e) => {
                  tracing::warn!(
                      event = "ui.watcher.create_failed",
                      error = %e,
                      "File watcher unavailable - falling back to polling"
                  );
                  return None;
              }
          };

          // Watch directory non-recursively (sessions are flat .json files)
          if let Err(e) = watcher.watch(sessions_dir, RecursiveMode::NonRecursive) {
              tracing::warn!(
                  event = "ui.watcher.watch_failed",
                  path = %sessions_dir.display(),
                  error = %e,
                  "Cannot watch sessions directory - falling back to polling"
              );
              return None;
          }

          tracing::info!(
              event = "ui.watcher.started",
              path = %sessions_dir.display()
          );

          Some(Self {
              _watcher: watcher,
              receiver: rx,
          })
      }

      /// Check for pending file events (non-blocking).
      ///
      /// Returns `true` if any relevant events (create/modify/remove of .json files)
      /// were detected since the last call.
      pub fn has_pending_events(&self) -> bool {
          loop {
              match self.receiver.try_recv() {
                  Ok(Ok(event)) => {
                      if Self::is_relevant_event(&event) {
                          tracing::debug!(
                              event = "ui.watcher.event_detected",
                              kind = ?event.kind,
                              paths = ?event.paths
                          );
                          // Drain remaining events and return true
                          while self.receiver.try_recv().is_ok() {}
                          return true;
                      }
                      // Not relevant, continue checking
                  }
                  Ok(Err(e)) => {
                      tracing::warn!(
                          event = "ui.watcher.event_error",
                          error = %e
                      );
                      // Continue checking - errors are non-fatal
                  }
                  Err(TryRecvError::Empty) => {
                      // No more events
                      return false;
                  }
                  Err(TryRecvError::Disconnected) => {
                      tracing::warn!(event = "ui.watcher.channel_disconnected");
                      return false;
                  }
              }
          }
      }

      /// Check if an event is relevant (create/modify/remove of .json files).
      fn is_relevant_event(event: &Event) -> bool {
          // Only care about create, modify, remove events
          let is_relevant_kind = matches!(
              event.kind,
              EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
          );

          if !is_relevant_kind {
              return false;
          }

          // Only care about .json files (session files)
          event.paths.iter().any(|p| {
              p.extension()
                  .and_then(|ext| ext.to_str())
                  .map(|ext| ext == "json")
                  .unwrap_or(false)
          })
      }
  }
  ```
- **PATTERN**: Follows kild-ui module style with doc comments, structured logging
- **GOTCHA**: Must store `_watcher` to keep it alive (same pattern as `_refresh_task`)
- **VALIDATE**: `cargo check -p kild-ui`

### Task 5: UPDATE `crates/kild-ui/src/main.rs`

- **ACTION**: ADD module declaration
- **IMPLEMENT**: Add `mod watcher;` after `mod refresh;`
- **VALIDATE**: `cargo check -p kild-ui`

### Task 6: UPDATE `crates/kild-ui/src/views/main_view.rs`

- **ACTION**: INTEGRATE file watcher with fallback to fast poll
- **IMPLEMENT**:

  1. Add import at top:
     ```rust
     use crate::watcher::SessionWatcher;
     ```

  2. Update MainView struct to store watcher:
     ```rust
     pub struct MainView {
         state: AppState,
         focus_handle: FocusHandle,
         /// Handle to the background refresh task. Must be stored to prevent cancellation.
         _refresh_task: Task<()>,
         /// Handle to the file watcher task. Must be stored to prevent cancellation.
         _watcher_task: Task<()>,
     }
     ```

  3. Update `MainView::new()` to create watcher and both tasks:
     ```rust
     pub fn new(cx: &mut Context<Self>) -> Self {
         // Get sessions directory for file watcher
         let config = kild_core::config::Config::new();
         let sessions_dir = config.sessions_dir();

         // Ensure sessions directory exists (create if needed for watcher)
         if !sessions_dir.exists() {
             if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
                 tracing::warn!(
                     event = "ui.sessions_dir.create_failed",
                     path = %sessions_dir.display(),
                     error = %e
                 );
             }
         }

         // Try to create file watcher
         let watcher = SessionWatcher::new(&sessions_dir);
         let has_watcher = watcher.is_some();

         // Determine poll interval based on watcher availability
         let poll_interval = if has_watcher {
             crate::refresh::POLL_INTERVAL  // 60s with watcher
         } else {
             crate::refresh::FAST_POLL_INTERVAL  // 5s fallback
         };

         // Slow poll task (60s with watcher, 5s without)
         let refresh_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
             tracing::debug!(
                 event = "ui.auto_refresh.started",
                 interval_secs = poll_interval.as_secs()
             );

             loop {
                 cx.background_executor().timer(poll_interval).await;

                 if let Err(e) = this.update(cx, |view, cx| {
                     tracing::debug!(event = "ui.auto_refresh.tick");
                     view.state.update_statuses_only();
                     cx.notify();
                 }) {
                     tracing::debug!(
                         event = "ui.auto_refresh.stopped",
                         reason = "view_dropped",
                         error = ?e
                     );
                     break;
                 }
             }
         });

         // File watcher task (checks for events frequently, cheap when no events)
         let watcher_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
             let Some(watcher) = watcher else {
                 tracing::debug!(event = "ui.watcher_task.skipped", reason = "no watcher");
                 return;
             };

             tracing::debug!(event = "ui.watcher_task.started");
             let mut last_refresh = std::time::Instant::now();

             loop {
                 // Check for events every 50ms (cheap - just channel poll)
                 cx.background_executor()
                     .timer(std::time::Duration::from_millis(50))
                     .await;

                 if let Err(e) = this.update(cx, |view, cx| {
                     if watcher.has_pending_events() {
                         // Debounce: only refresh if enough time has passed
                         if last_refresh.elapsed() > crate::refresh::DEBOUNCE_INTERVAL {
                             tracing::info!(event = "ui.watcher.refresh_triggered");
                             view.state.refresh_sessions();
                             last_refresh = std::time::Instant::now();
                             cx.notify();
                         }
                     }
                 }) {
                     tracing::debug!(
                         event = "ui.watcher_task.stopped",
                         reason = "view_dropped",
                         error = ?e
                     );
                     break;
                 }
             }
         });

         Self {
             state: AppState::new(),
             focus_handle: cx.focus_handle(),
             _refresh_task: refresh_task,
             _watcher_task: watcher_task,
         }
     }
     ```

- **MIRROR**: `crates/kild-ui/src/views/main_view.rs:134-161` for task spawning pattern
- **GOTCHA**: Both tasks must be stored in struct fields to prevent cancellation
- **GOTCHA**: Use `refresh_sessions()` (full reload) for watcher events, `update_statuses_only()` for poll (cheaper)
- **VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

## Testing Strategy

### Manual Testing

1. **Start UI**: `cargo run --bin kild-ui`
2. **In another terminal, run CLI commands**:
   ```bash
   kild create test-watcher --note "Testing file watcher"
   # UI should update within ~100ms

   kild stop test-watcher
   # UI should show stopped status within ~100ms

   kild destroy test-watcher --force
   # UI should remove kild within ~100ms
   ```
3. **Kill an agent process directly**:
   ```bash
   kill <agent-pid>
   # UI should update within 60s (poll fallback)
   ```
4. **Verify graceful degradation** (optional):
   - Temporarily make sessions dir unreadable: `chmod 000 ~/.kild/sessions`
   - Start UI - should fall back to 5s polling with warning log
   - Restore: `chmod 755 ~/.kild/sessions`

### Unit Tests

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `crates/kild-ui/src/watcher.rs` | `is_relevant_event` filters correctly | Only .json create/modify/remove trigger refresh |

**Note**: Full integration tests are deferred - manual testing covers the critical paths.

### Edge Cases Checklist

- [ ] Sessions directory doesn't exist on startup → Created automatically, watcher starts
- [ ] Sessions directory permissions denied → Falls back to 5s poll with warning log
- [ ] Watcher channel disconnects → Warning logged, stops checking (poll still works)
- [ ] Rapid file changes (bulk operation) → Debounced to single refresh per 100ms
- [ ] Non-.json files modified → Ignored (e.g., temp files, .DS_Store)
- [ ] UI started before any sessions exist → Works, updates when first session created

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy -p kild-ui -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: TYPE_CHECK

```bash
cargo check -p kild-ui
```

**EXPECT**: Exit 0, compiles successfully

### Level 3: BUILD

```bash
cargo build -p kild-ui
```

**EXPECT**: Exit 0, binary builds successfully

### Level 4: FULL_WORKSPACE

```bash
cargo fmt --check && cargo clippy --all -- -D warnings && cargo test --all && cargo build --all
```

**EXPECT**: All checks pass, all tests pass, all crates build

### Level 5: MANUAL_VALIDATION

1. Start kild-ui: `cargo run --bin kild-ui`
2. In separate terminal: `kild create test-watcher`
3. Verify: UI updates within ~1 second (not 5 seconds)
4. Run: `kild destroy test-watcher --force`
5. Verify: Kild disappears from UI within ~1 second

---

## Acceptance Criteria

- [ ] UI updates within ~100ms of CLI operations (create, stop, open, destroy)
- [ ] UI updates within 60s when agent process dies externally (kill -9)
- [ ] If file watching fails, falls back to 5s polling with warning log
- [ ] No regression in existing functionality (bulk ops, project filtering, etc.)
- [ ] Level 1-4 validation commands pass with exit 0
- [ ] CPU usage lower than current 5s polling approach (fewer wake-ups)

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (fmt + clippy) passes
- [ ] Level 2: Type check passes
- [ ] Level 3: Build succeeds
- [ ] Level 4: Full workspace check passes
- [ ] Manual validation confirms instant updates
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| notify crate doesn't work on some macOS versions | LOW | HIGH | Graceful fallback to 5s poll; notify uses FSEvents which is stable |
| File watcher misses events (known notify edge cases) | LOW | LOW | 60s poll catches anything missed; acceptable for rare cases |
| Higher memory usage from watcher thread | LOW | LOW | notify uses platform APIs efficiently; thread overhead negligible |
| Breaking change in notify 8.x API | LOW | MED | Pin to exact version 8.0; explicit in Cargo.toml |

---

## Notes

- **Why 60s poll vs removing poll entirely**: Process deaths (kill -9) don't create file events. The 60s poll is a safety net, not the primary mechanism. 60s is acceptable latency for the rare "external kill" case.

- **Why manual debounce vs notify-debouncer crates**: Adding notify-debouncer-mini would add another dependency. The manual debounce (100ms check against last refresh time) is simple, easy to understand, and covers our use case perfectly.

- **Why `refresh_sessions()` for watcher vs `update_statuses_only()`**: File events indicate actual session changes (create/destroy), so a full refresh is appropriate. The poll uses `update_statuses_only()` for efficiency since it's just catching status changes.

- **Thread safety**: The watcher is created on the main thread and its receiver is polled from a GPUI background task. The mpsc channel is thread-safe. The watcher itself is Send+Sync.
