# Feature: Command Dispatch & Migration (Phase 2d + 2e)

## Summary

Wire the existing `Store::dispatch()` trait to route `Command` enum variants to their corresponding handler functions in kild-core. Then migrate the UI's `actions.rs` to dispatch commands instead of calling handlers directly. The CLI is intentionally excluded from migration — its command handlers have CLI-specific concerns (arg parsing, output formatting, safety prompts) that don't map cleanly to fire-and-forget dispatch.

## User Story

As a kild developer
I want business operations routed through a command dispatch layer
So that all state mutations flow through a single traceable path

## Problem Statement

The `Command` enum and `Store` trait exist as type-only definitions (Phase 2a-2c). No dispatch logic connects them to the existing handler functions. The UI calls handlers directly through `actions.rs`, bypassing the command pattern entirely.

## Solution Statement

1. Define a `DispatchError` enum in `kild-core/src/state/errors.rs` that wraps `SessionError` and `ProjectError`
2. Implement `Store` for a new `CoreStore` struct in `kild-core/src/state/dispatch.rs` that routes each `Command` variant to its handler
3. Migrate UI `actions.rs` session operations to build `Command` variants and call `CoreStore::dispatch()`
4. Keep project operations in `actions.rs` as-is — they have UI-specific persistence logic that doesn't match the core handler signatures

## Metadata

| Field            | Value                                              |
| ---------------- | -------------------------------------------------- |
| Type             | REFACTOR                                           |
| Complexity       | MEDIUM                                             |
| Systems Affected | kild-core/state, kild-ui/actions                   |
| Dependencies     | None (builds on existing Phase 2a-2c types)        |
| Estimated Tasks  | 5                                                  |

---

## UX Design

### Before State

```
CLI commands.rs ──────► session_ops::create_session()
                        session_ops::destroy_session()
                        session_ops::list_sessions()
                        ...

UI  actions.rs  ──────► session_ops::create_session()
                        session_ops::destroy_session()
                        session_ops::list_sessions()
                        ...
                        load_projects() / save_projects()  (manual persistence)

Command enum ──── (disconnected, types only)
Store trait  ──── (disconnected, no impl)
```

### After State

```
CLI commands.rs ──────► session_ops::* (unchanged, CLI-specific concerns)

UI  actions.rs  ──────► CoreStore::dispatch(Command::CreateKild { .. })
                        CoreStore::dispatch(Command::DestroyKild { .. })
                        CoreStore::dispatch(Command::StopKild { .. })
                        CoreStore::dispatch(Command::OpenKild { .. })
                        CoreStore::dispatch(Command::RefreshSessions)
                        load_projects() / save_projects()  (project ops unchanged)

CoreStore ─── dispatch() ─── match cmd {
                                CreateKild    => session_ops::create_session()
                                DestroyKild   => session_ops::destroy_session()
                                OpenKild      => session_ops::open_session()
                                StopKild      => session_ops::stop_session()
                                CompleteKild  => session_ops::complete_session()
                                RefreshSessions => session_ops::list_sessions()
                                AddProject    => (reserved, not wired yet)
                                RemoveProject => (reserved, not wired yet)
                                SelectProject => (reserved, not wired yet)
                              }
```

---

## Mandatory Reading

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-core/src/state/types.rs` | 1-48 | Command enum — the variants dispatch must match |
| P0 | `crates/kild-core/src/state/store.rs` | 1-19 | Store trait contract |
| P0 | `crates/kild-core/src/sessions/handler.rs` | 44-47, 208, 259, 410, 954, 1068 | Handler signatures dispatch calls |
| P0 | `crates/kild-ui/src/actions.rs` | 20-171 | Current UI handler calls to migrate |
| P1 | `crates/kild-core/src/sessions/errors.rs` | 1-69 | SessionError variants |
| P1 | `crates/kild-core/src/projects/errors.rs` | 1-30 | ProjectError variants |
| P1 | `crates/kild-core/src/state/mod.rs` | all | Module structure to extend |
| P1 | `crates/kild-core/src/lib.rs` | 26-44 | Public re-exports to extend |
| P2 | `crates/kild-core/src/errors/mod.rs` | 6-14 | KildError trait pattern |

---

## Patterns to Mirror

**ERROR_HANDLING:**
```rust
// SOURCE: crates/kild-core/src/sessions/errors.rs:3-69
// COPY THIS PATTERN for DispatchError:
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' already exists")]
    AlreadyExists { name: String },
    // ...
}

impl KildError for SessionError {
    fn error_code(&self) -> &'static str { /* ... */ }
    fn is_user_error(&self) -> bool { /* ... */ }
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-core/src/sessions/handler.rs:44-50
// COPY THIS PATTERN for dispatch logging:
info!(event = "core.session.create_started", branch = request.branch, agent = agent);
// ...
info!(event = "core.session.create_completed", session_id = session.id);
error!(event = "core.session.create_failed", error = %e);
```

**MODULE_RE-EXPORT:**
```rust
// SOURCE: crates/kild-core/src/state/mod.rs:1-6
pub mod errors;
pub mod store;
pub mod types;

pub use store::Store;
pub use types::Command;
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-core/src/state/errors.rs` | UPDATE | Define DispatchError wrapping SessionError + ProjectError |
| `crates/kild-core/src/state/dispatch.rs` | CREATE | CoreStore struct implementing Store trait |
| `crates/kild-core/src/state/mod.rs` | UPDATE | Add dispatch module, re-export CoreStore + DispatchError |
| `crates/kild-core/src/lib.rs` | UPDATE | Re-export CoreStore and DispatchError |
| `crates/kild-ui/src/actions.rs` | UPDATE | Migrate session ops to use CoreStore::dispatch() |

---

## NOT Building (Scope Limits)

- **CLI migration** — CLI commands have arg parsing, output formatting, safety prompts (destroy confirmation), and bulk operations that don't fit dispatch's `Result<(), Error>` contract. CLI stays as-is.
- **Project command dispatch** — AddProject/RemoveProject/SelectProject in the UI have persistence logic in actions.rs that loads/saves the full projects file. The ProjectManager methods in core don't persist. Wiring these requires either adding persistence to core handlers or a different approach. Dispatch will log + return `Ok(())` for project commands as a no-op placeholder.
- **Async dispatch** — The Store trait is sync. No async changes.
- **Event emission** — No event bus. Tracing logs only.
- **Undo** — Future phase, not needed now.

---

## Step-by-Step Tasks

### Task 1: UPDATE `crates/kild-core/src/state/errors.rs` — Define DispatchError

- **ACTION**: Replace placeholder comment with `DispatchError` enum
- **IMPLEMENT**:
  ```rust
  use crate::errors::KildError;
  use crate::projects::errors::ProjectError;
  use crate::sessions::errors::SessionError;

  #[derive(Debug, thiserror::Error)]
  pub enum DispatchError {
      #[error(transparent)]
      Session(#[from] SessionError),
      #[error(transparent)]
      Project(#[from] ProjectError),
      #[error("Config error: {0}")]
      Config(String),
  }

  impl KildError for DispatchError {
      fn error_code(&self) -> &'static str {
          match self {
              DispatchError::Session(e) => e.error_code(),
              DispatchError::Project(e) => e.error_code(),
              DispatchError::Config(_) => "DISPATCH_CONFIG_ERROR",
          }
      }

      fn is_user_error(&self) -> bool {
          match self {
              DispatchError::Session(e) => e.is_user_error(),
              DispatchError::Project(e) => e.is_user_error(),
              DispatchError::Config(_) => true,
          }
      }
  }
  ```
- **MIRROR**: `crates/kild-core/src/sessions/errors.rs:3-112` for error + KildError pattern
- **TESTS**: Error display, error_code delegation, is_user_error delegation, From conversions
- **VALIDATE**: `cargo build -p kild-core && cargo test -p kild-core`

### Task 2: CREATE `crates/kild-core/src/state/dispatch.rs` — CoreStore implementation

- **ACTION**: Create CoreStore struct that implements Store trait
- **IMPLEMENT**:
  ```rust
  use tracing::{debug, error, info};

  use crate::config::KildConfig;
  use crate::sessions::handler as session_ops;
  use crate::state::errors::DispatchError;
  use crate::state::store::Store;
  use crate::state::types::Command;

  /// Default Store implementation that routes commands to kild-core handlers.
  ///
  /// Loads config on construction. Session operations delegate to `sessions::handler`.
  /// Project operations are not yet wired (logged and return Ok).
  pub struct CoreStore {
      config: KildConfig,
  }

  impl CoreStore {
      pub fn new(config: KildConfig) -> Self {
          Self { config }
      }
  }

  impl Store for CoreStore {
      type Error = DispatchError;

      fn dispatch(&mut self, cmd: Command) -> Result<(), DispatchError> {
          debug!(event = "core.state.dispatch_started", command = ?cmd);

          match cmd {
              Command::CreateKild { branch, agent, note, project_path } => {
                  let request = match project_path {
                      Some(path) => crate::sessions::types::CreateSessionRequest::with_project_path(
                          branch, agent, note, path,
                      ),
                      None => crate::sessions::types::CreateSessionRequest::new(branch, agent, note),
                  };
                  session_ops::create_session(request, &self.config)?;
                  Ok(())
              }
              Command::DestroyKild { branch, force } => {
                  session_ops::destroy_session(&branch, force)?;
                  Ok(())
              }
              Command::OpenKild { branch, agent } => {
                  session_ops::open_session(&branch, agent)?;
                  Ok(())
              }
              Command::StopKild { branch } => {
                  session_ops::stop_session(&branch)?;
                  Ok(())
              }
              Command::CompleteKild { branch, force } => {
                  session_ops::complete_session(&branch, force)?;
                  Ok(())
              }
              Command::RefreshSessions => {
                  session_ops::list_sessions()?;
                  Ok(())
              }
              Command::AddProject { .. }
              | Command::RemoveProject { .. }
              | Command::SelectProject { .. } => {
                  debug!(event = "core.state.dispatch_skipped", reason = "project commands not yet wired");
                  Ok(())
              }
          }
      }
  }
  ```
- **MIRROR**: `crates/kild-core/src/state/store.rs:16-19` for trait impl pattern
- **TESTS**:
  - `test_core_store_implements_store_trait` — construct CoreStore, verify it compiles
  - `test_core_store_project_commands_return_ok` — verify AddProject/RemoveProject/SelectProject return Ok (no-op)
- **GOTCHA**: `open_session` loads its own `KildConfig` internally (handler.rs:962-975), so the config passed to CoreStore is only used by `create_session`. This is fine — handler-internal config loading is the existing pattern.
- **VALIDATE**: `cargo build -p kild-core && cargo test -p kild-core`

### Task 3: UPDATE `crates/kild-core/src/state/mod.rs` — Wire dispatch module

- **ACTION**: Add dispatch module and re-exports
- **IMPLEMENT**:
  ```rust
  pub mod dispatch;
  pub mod errors;
  pub mod store;
  pub mod types;

  pub use dispatch::CoreStore;
  pub use errors::DispatchError;
  pub use store::Store;
  pub use types::Command;
  ```
- **VALIDATE**: `cargo build -p kild-core`

### Task 4: UPDATE `crates/kild-core/src/lib.rs` — Re-export CoreStore and DispatchError

- **ACTION**: Add CoreStore and DispatchError to the public API
- **IMPLEMENT**: Add to the existing `pub use state::` line:
  ```rust
  pub use state::{Command, CoreStore, DispatchError, Store};
  ```
- **VALIDATE**: `cargo build --all`

### Task 5: UPDATE `crates/kild-ui/src/actions.rs` — Migrate session ops to dispatch

- **ACTION**: Replace direct `session_ops::*` calls with `CoreStore::dispatch()` for session operations
- **SCOPE**: Only migrate `create_kild`, `destroy_kild`, `open_kild`, `stop_kild`. Keep `refresh_sessions` as-is (it returns data). Keep all project operations as-is (UI-specific persistence logic).
- **IMPLEMENT** (example for `create_kild`):
  ```rust
  use kild_core::{Command, CoreStore, KildConfig};

  pub fn create_kild(
      branch: &str,
      agent: &str,
      note: Option<String>,
      project_path: Option<PathBuf>,
  ) -> Result<Session, String> {
      // ... existing validation and logging ...

      let config = match KildConfig::load_hierarchy() {
          Ok(c) => c,
          Err(e) => { /* existing error handling */ }
      };

      let mut store = CoreStore::new(config);
      store.dispatch(Command::CreateKild {
          branch: branch.to_string(),
          agent: Some(agent.to_string()),
          note,
          project_path,
      }).map_err(|e| e.to_string())?;

      // Problem: dispatch returns () but we need Session.
      // Solution: Call session_ops::create_session directly — dispatch
      // doesn't replace handlers that return data the caller needs.
  ```
- **DECISION**: After analysis, `create_kild` and `open_kild` need the `Session` return value. `refresh_sessions` needs `Vec<Session>`. The `Store::dispatch()` returns `()`. There are two options:
  1. **Keep actions.rs calling handlers directly** for operations that need return values. Only migrate void-returning operations (`destroy_kild`, `stop_kild`).
  2. **Don't migrate actions.rs yet** — wait until Store trait supports richer return types or state storage.

  **Recommendation: Option 1** — migrate `destroy_kild` and `stop_kild` (which return `Result<(), String>`) to use dispatch. Keep `create_kild`, `open_kild`, and `refresh_sessions` calling handlers directly since they need return values.

- **IMPLEMENT for destroy_kild**:
  ```rust
  pub fn destroy_kild(branch: &str, force: bool) -> Result<(), String> {
      tracing::info!(event = "ui.destroy_kild.started", branch = branch, force = force);

      let config = KildConfig::load_hierarchy().unwrap_or_default();
      let mut store = CoreStore::new(config);

      match store.dispatch(Command::DestroyKild {
          branch: branch.to_string(),
          force,
      }) {
          Ok(()) => {
              tracing::info!(event = "ui.destroy_kild.completed", branch = branch);
              Ok(())
          }
          Err(e) => {
              tracing::error!(event = "ui.destroy_kild.failed", branch = branch, error = %e);
              Err(e.to_string())
          }
      }
  }
  ```
- **IMPLEMENT for stop_kild**: Same pattern as destroy_kild
- **IMPORTS**: Replace `use kild_core::session_ops` with `use kild_core::{Command, CoreStore}` for the migrated functions. Keep `session_ops` import for functions that still call handlers directly.
- **GOTCHA**: `destroy_session` and `stop_session` don't need `KildConfig` (they create `Config::new()` internally). CoreStore still needs a config for other commands, so pass `unwrap_or_default()`.
- **VALIDATE**: `cargo build --all && cargo test --all && cargo clippy --all -- -D warnings`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `crates/kild-core/src/state/errors.rs` | Display, error_code, is_user_error, From<SessionError>, From<ProjectError> | DispatchError |
| `crates/kild-core/src/state/dispatch.rs` | Trait impl compiles, project commands no-op | CoreStore |

### Edge Cases Checklist

- [ ] DispatchError::from(SessionError) preserves error code
- [ ] DispatchError::from(ProjectError) preserves error code
- [ ] DispatchError::Config has correct error code
- [ ] CoreStore project commands return Ok (not Err)
- [ ] CoreStore compiles with Store trait bound

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p kild-core -- state
```

**EXPECT**: All state module tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All 95+ tests pass, build succeeds (no regressions)

---

## Acceptance Criteria

- [ ] `DispatchError` enum defined with `From<SessionError>` and `From<ProjectError>`
- [ ] `DispatchError` implements `KildError` trait
- [ ] `CoreStore` struct implements `Store` trait
- [ ] `CoreStore::dispatch()` routes session commands to existing handlers
- [ ] `CoreStore` and `DispatchError` re-exported from `kild-core`
- [ ] UI `destroy_kild` and `stop_kild` use `CoreStore::dispatch()`
- [ ] All existing tests pass (no regressions)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all -- -D warnings` passes

---

## Completion Checklist

- [ ] All tasks completed in dependency order (1→2→3→4→5)
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis passes
- [ ] Level 2: State module tests pass
- [ ] Level 3: Full suite + build succeeds
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Store's `()` return type limits migration | HIGH | LOW | Only migrate void-returning operations. Document limitation for future phase. |
| Handler-internal config loading makes CoreStore's config field partially unused | LOW | LOW | Document that only create_session uses it. Other handlers load config internally. |
| Bulk operations (open --all, stop --all) don't fit Command pattern | MED | LOW | Out of scope. Bulk operations stay as loops in CLI/UI. |

---

## Notes

**Why not migrate the CLI**: CLI commands have concerns that don't map to dispatch:
- Arg parsing with overrides (`--terminal`, `--startup-command`, `--flags`)
- Safety confirmation prompts (`get_destroy_safety_info` + user prompt)
- Rich output formatting (tables, JSON, colored text)
- Bulk operations as loops over all sessions
- Commands not in the Command enum (`cd`, `code`, `focus`, `diff`, `commits`, `status`, `cleanup`, `health`)

The CLI would need a fundamentally different dispatch contract to benefit.

**Why not migrate project operations in UI**: The UI's `add_project`, `remove_project`, `set_active_project` in `actions.rs` do their own load/validate/mutate/save cycle against the projects JSON file. The `ProjectManager` methods in core only operate on in-memory state and don't persist. Wiring these through dispatch would either require adding persistence to core's project handlers (scope creep) or making dispatch aware of persistence (breaks separation). Better handled in a future phase that unifies project persistence.

**Store `()` return type**: The current Store trait returns `Result<(), Error>`. Operations like `CreateKild` discard the `Session` return value. This is the documented contract from Phase 2c. A future phase could add a `DispatchResult` type or generic return, but that's out of scope.
