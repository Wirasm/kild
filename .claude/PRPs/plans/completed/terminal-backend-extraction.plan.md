# Feature: Extract Terminal Backends into Separate Modules

## Summary

Refactor the monolithic `operations.rs` (777 lines) by extracting terminal-specific logic into self-contained backend modules following the established `AgentBackend` trait pattern. Each terminal (Ghostty, iTerm, Terminal.app) will have its own module implementing a `TerminalBackend` trait, eliminating the 5 match statements on `TerminalType` and making it trivial to add new terminals.

## User Story

As a developer adding support for new terminals (Alacritty, Warp, Kitty, WezTerm)
I want each terminal's logic isolated in its own file
So that I can implement a new backend without understanding or modifying existing terminal code

## Problem Statement

The terminal module has 777 lines in `operations.rs` with all terminal-specific logic mixed together:
- iTerm AppleScript templates and execution
- Terminal.app AppleScript templates and execution
- Ghostty spawn/close logic (uses CLI, not AppleScript)
- Detection logic scattered across multiple functions
- 5 match statements on `TerminalType` scattered across the file

Adding a new terminal requires editing the monolithic file, adding match arms in multiple places, and understanding all existing terminal logic.

## Solution Statement

Extract terminal-specific logic into a trait-based backend system mirroring the existing `AgentBackend` pattern:
1. Define `TerminalBackend` trait with methods for availability checking, spawning, and closing
2. Create backend implementations: `GhosttyBackend`, `ITermBackend`, `TerminalAppBackend`
3. Use registry pattern for backend lookup
4. Replace match statements with polymorphic trait dispatch
5. Move utility functions to `common/` submodule

## Metadata

| Field            | Value                                   |
| ---------------- | --------------------------------------- |
| Type             | REFACTOR                                |
| Complexity       | MEDIUM                                  |
| Systems Affected | terminal module                         |
| Dependencies     | none (internal refactor)                |
| Estimated Tasks  | 14                                      |

---

## UX Design

### Before State
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────────────┐         ┌─────────────────────┐                     ║
║   │  spawn_terminal()   │         │  operations.rs      │                     ║
║   │  in handler.rs      │ ──────► │  (777 lines)        │                     ║
║   └─────────────────────┘         │                     │                     ║
║                                   │  - ITERM_SCRIPT     │                     ║
║                                   │  - TERMINAL_SCRIPT  │                     ║
║                                   │  - detect_terminal  │                     ║
║                                   │  - build_spawn_cmd  │                     ║
║                                   │  - execute_spawn    │                     ║
║                                   │  - close_terminal   │                     ║
║                                   │  - shell_escape     │                     ║
║                                   │  - applescript_esc  │                     ║
║                                   │  + 5 match arms     │                     ║
║                                   └─────────────────────┘                     ║
║                                                                               ║
║   DEVELOPER_EXPERIENCE: Add Alacritty support requires:                       ║
║   1. Edit monolithic operations.rs                                            ║
║   2. Add match arm in build_spawn_command()                                   ║
║   3. Add match arm in execute_spawn_script()                                  ║
║   4. Add match arm in close_terminal_window()                                 ║
║   5. Add match arm in detect_terminal()                                       ║
║   6. Update TerminalType enum in types.rs                                     ║
║                                                                               ║
║   PAIN_POINT: Must understand 777 lines of mixed terminal logic               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────────────┐         ┌─────────────────────┐                     ║
║   │  spawn_terminal()   │         │    registry.rs      │                     ║
║   │  in handler.rs      │ ──────► │  get_backend(type)  │                     ║
║   └─────────────────────┘         └──────────┬──────────┘                     ║
║                                              │                                ║
║                           ┌──────────────────┼──────────────────┐             ║
║                           │                  │                  │             ║
║                           ▼                  ▼                  ▼             ║
║               ┌───────────────┐  ┌───────────────┐  ┌───────────────┐         ║
║               │ ghostty.rs    │  │ iterm.rs      │  │ terminal_app  │         ║
║               │ (~100 lines)  │  │ (~100 lines)  │  │ .rs (~100 ln) │         ║
║               │               │  │               │  │               │         ║
║               │ impl Backend  │  │ impl Backend  │  │ impl Backend  │         ║
║               │ - is_avail()  │  │ - is_avail()  │  │ - is_avail()  │         ║
║               │ - spawn()     │  │ - spawn()     │  │ - spawn()     │         ║
║               │ - close()     │  │ - close()     │  │ - close()     │         ║
║               └───────────────┘  └───────────────┘  └───────────────┘         ║
║                                                                               ║
║   ┌─────────────────────┐                                                     ║
║   │  common/escape.rs   │  ◄── Shared utilities (shell_escape, applescript)   ║
║   └─────────────────────┘                                                     ║
║                                                                               ║
║   DEVELOPER_EXPERIENCE: Add Alacritty support requires:                       ║
║   1. Create backends/alacritty.rs (~100 lines)                                ║
║   2. Implement TerminalBackend trait                                          ║
║   3. Add variant to TerminalType enum                                         ║
║   4. Register in backends/mod.rs                                              ║
║                                                                               ║
║   VALUE_ADD: Self-contained, discoverable, testable in isolation              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes
| Location | Before | After | Developer Impact |
|----------|--------|-------|------------------|
| `operations.rs` | 777 lines, all terminals mixed | ~50 lines, delegates to registry | Simpler to understand |
| `backends/` | N/A | 3 files, ~100 lines each | Clear where terminal logic lives |
| Adding terminal | Edit 4+ match arms | Create 1 file, implement trait | 5x faster to add |
| Testing | Test entire operations.rs | Test each backend in isolation | Faster, focused tests |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards-core/src/agents/traits.rs` | all | Trait pattern to MIRROR exactly |
| P0 | `crates/shards-core/src/agents/backends/claude.rs` | all | Backend implementation pattern |
| P0 | `crates/shards-core/src/agents/registry.rs` | all | Registry pattern to MIRROR |
| P0 | `crates/shards-core/src/terminal/operations.rs` | all | Source code being refactored |
| P1 | `crates/shards-core/src/terminal/types.rs` | all | TerminalType enum definition |
| P1 | `crates/shards-core/src/terminal/errors.rs` | all | Error types to use |
| P1 | `crates/shards-core/src/terminal/handler.rs` | all | Public API that calls operations |
| P2 | `crates/shards-core/src/agents/backends/mod.rs` | all | Module structure pattern |

---

## Patterns to Mirror

**TRAIT_DEFINITION:**
```rust
// SOURCE: crates/shards-core/src/agents/traits.rs:7-33
// COPY THIS PATTERN:
pub trait AgentBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn default_command(&self) -> &'static str;
    fn process_patterns(&self) -> Vec<String>;
    fn command_patterns(&self) -> Vec<String> {
        vec![self.default_command().to_string()]
    }
}
```

**BACKEND_STRUCT:**
```rust
// SOURCE: crates/shards-core/src/agents/backends/claude.rs:6-28
// COPY THIS PATTERN:
pub struct ClaudeBackend;

impl AgentBackend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }
    // ... simple method implementations
}
```

**REGISTRY_PATTERN:**
```rust
// SOURCE: crates/shards-core/src/agents/registry.rs:10-46
// COPY THIS PATTERN:
static REGISTRY: LazyLock<TerminalRegistry> = LazyLock::new(TerminalRegistry::new);

struct TerminalRegistry {
    backends: HashMap<TerminalType, Box<dyn TerminalBackend>>,
}

impl TerminalRegistry {
    fn new() -> Self {
        let mut backends = HashMap::new();
        backends.insert(TerminalType::Ghostty, Box::new(GhosttyBackend));
        // ...
        Self { backends }
    }
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: crates/shards-core/src/terminal/errors.rs:3-31
// COPY THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("...")]
    VariantName { field: String },
}
```

**MODULE_EXPORTS:**
```rust
// SOURCE: crates/shards-core/src/agents/backends/mod.rs:1-12
// COPY THIS PATTERN:
mod aether;
mod claude;
// ...
pub use aether::AetherBackend;
pub use claude::ClaudeBackend;
// ...
```

**TEST_STRUCTURE:**
```rust
// SOURCE: crates/shards-core/src/agents/backends/claude.rs:30-66
// COPY THIS PATTERN:
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_name() {
        let backend = ClaudeBackend;
        assert_eq!(backend.name(), "claude");
    }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards-core/src/terminal/traits.rs` | CREATE | Define `TerminalBackend` trait |
| `crates/shards-core/src/terminal/common/mod.rs` | CREATE | Module for shared utilities |
| `crates/shards-core/src/terminal/common/escape.rs` | CREATE | shell_escape, applescript_escape |
| `crates/shards-core/src/terminal/backends/mod.rs` | CREATE | Backend module exports |
| `crates/shards-core/src/terminal/backends/ghostty.rs` | CREATE | Ghostty-specific spawn/close |
| `crates/shards-core/src/terminal/backends/iterm.rs` | CREATE | iTerm AppleScript logic |
| `crates/shards-core/src/terminal/backends/terminal_app.rs` | CREATE | Terminal.app AppleScript logic |
| `crates/shards-core/src/terminal/registry.rs` | CREATE | Backend lookup and detection |
| `crates/shards-core/src/terminal/operations.rs` | UPDATE | Simplify to use registry dispatch |
| `crates/shards-core/src/terminal/mod.rs` | UPDATE | Add new module exports |
| `crates/shards-core/src/terminal/handler.rs` | UPDATE | Minor: use registry for detection |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **NOT adding new terminals** - Only extracting existing (Ghostty, iTerm, Terminal.app)
- **NOT changing public API** - `spawn_terminal()`, `close_terminal()` signatures unchanged
- **NOT changing TerminalType enum** - Keep existing 4 variants (ITerm, TerminalApp, Ghostty, Native)
- **NOT moving handler.rs logic** - Keep high-level orchestration in handler.rs
- **NOT platform abstraction** - macOS-only for now, `#[cfg(target_os = "macos")]` stays
- **NOT registry public API** - Registry is internal, not exposed publicly

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `crates/shards-core/src/terminal/traits.rs`

- **ACTION**: CREATE trait definition file
- **IMPLEMENT**: Define `TerminalBackend` trait with:
  ```rust
  pub trait TerminalBackend: Send + Sync {
      /// The canonical name (e.g., "ghostty", "iterm")
      fn name(&self) -> &'static str;

      /// Display name (e.g., "Ghostty", "iTerm2")
      fn display_name(&self) -> &'static str;

      /// Check if this terminal is available on the system
      fn is_available(&self) -> bool;

      /// Execute spawn and return window ID
      fn execute_spawn(
          &self,
          config: &SpawnConfig,
          window_title: Option<&str>,
      ) -> Result<Option<String>, TerminalError>;

      /// Close a terminal window
      fn close_window(&self, window_id: Option<&str>) -> Result<(), TerminalError>;
  }
  ```
- **MIRROR**: `crates/shards-core/src/agents/traits.rs`
- **IMPORTS**:
  - `use crate::terminal::{errors::TerminalError, types::SpawnConfig}`
- **GOTCHA**: Include `Send + Sync` bounds for thread safety
- **VALIDATE**: `cargo check -p shards-core`

### Task 2: CREATE `crates/shards-core/src/terminal/common/mod.rs`

- **ACTION**: CREATE common utilities module
- **IMPLEMENT**:
  ```rust
  pub mod escape;
  ```
- **VALIDATE**: `cargo check -p shards-core`

### Task 3: CREATE `crates/shards-core/src/terminal/common/escape.rs`

- **ACTION**: EXTRACT escape utilities from operations.rs
- **IMPLEMENT**: Move these functions:
  - `shell_escape(s: &str) -> String` (operations.rs:181-183)
  - `applescript_escape(s: &str) -> String` (operations.rs:185-190)
  - `escape_regex(s: &str) -> String` (operations.rs:84-96)
  - `build_cd_command(working_directory: &Path, command: &str) -> String` (operations.rs:75-81)
- **MIRROR**: Keep exact same implementation from operations.rs
- **TESTS**: Move related tests from operations.rs (test_shell_escape, test_applescript_escape, test_escape_regex, test_build_cd_command)
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core escape`

### Task 4: CREATE `crates/shards-core/src/terminal/backends/mod.rs`

- **ACTION**: CREATE backends module exports
- **IMPLEMENT**:
  ```rust
  //! Terminal backend implementations.

  mod ghostty;
  mod iterm;
  mod terminal_app;

  pub use ghostty::GhosttyBackend;
  pub use iterm::ITermBackend;
  pub use terminal_app::TerminalAppBackend;
  ```
- **MIRROR**: `crates/shards-core/src/agents/backends/mod.rs`
- **VALIDATE**: `cargo check -p shards-core` (will fail until backends implemented)

### Task 5: CREATE `crates/shards-core/src/terminal/backends/ghostty.rs`

- **ACTION**: EXTRACT Ghostty logic into backend
- **IMPLEMENT**:
  - `GhosttyBackend` struct implementing `TerminalBackend`
  - `is_available()`: Check `app_exists_macos("Ghostty")`
  - `execute_spawn()`: Extract from operations.rs:224-283 (Ghostty block in execute_spawn_script)
  - `close_window()`: Extract from operations.rs:385-427 (Ghostty pkill logic)
- **SOURCE**: operations.rs lines 224-283 (spawn), 385-427 (close)
- **IMPORTS**:
  - `use crate::terminal::{common::escape::*, errors::TerminalError, traits::TerminalBackend, types::SpawnConfig}`
  - `use tracing::{debug, warn}`
  - `use std::process::Command`
- **GOTCHA**: Ghostty uses CLI (`open -na Ghostty.app --args`), NOT AppleScript
- **GOTCHA**: Close uses `pkill -f` with regex-escaped session ID
- **TESTS**: Test trait methods, spawn command construction
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core ghostty`

### Task 6: CREATE `crates/shards-core/src/terminal/backends/iterm.rs`

- **ACTION**: EXTRACT iTerm logic into backend
- **IMPLEMENT**:
  - `ITermBackend` struct implementing `TerminalBackend`
  - Move `ITERM_SCRIPT` constant (operations.rs:6-13)
  - Move `ITERM_CLOSE_SCRIPT` constant (operations.rs:22-28)
  - `is_available()`: Check `app_exists_macos("iTerm")`
  - `execute_spawn()`: Build and execute AppleScript
  - `close_window()`: Build and execute close AppleScript
- **SOURCE**: operations.rs lines 6-13 (ITERM_SCRIPT), 22-28 (ITERM_CLOSE_SCRIPT)
- **IMPORTS**:
  - `use crate::terminal::{common::escape::*, errors::TerminalError, traits::TerminalBackend, types::SpawnConfig}`
  - `use tracing::debug`
  - `use std::process::Command`
- **PATTERN**: Extract osascript execution logic
- **TESTS**: Test trait methods, AppleScript construction
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core iterm`

### Task 7: CREATE `crates/shards-core/src/terminal/backends/terminal_app.rs`

- **ACTION**: EXTRACT Terminal.app logic into backend
- **IMPLEMENT**:
  - `TerminalAppBackend` struct implementing `TerminalBackend`
  - Move `TERMINAL_SCRIPT` constant (operations.rs:15-19)
  - Move `TERMINAL_CLOSE_SCRIPT` constant (operations.rs:30-36)
  - `is_available()`: Check `app_exists_macos("Terminal")`
  - `execute_spawn()`: Build and execute AppleScript
  - `close_window()`: Build and execute close AppleScript
- **SOURCE**: operations.rs lines 15-19 (TERMINAL_SCRIPT), 30-36 (TERMINAL_CLOSE_SCRIPT)
- **IMPORTS**: Same as iterm.rs
- **PATTERN**: Nearly identical to iterm.rs, just different AppleScript
- **TESTS**: Test trait methods, AppleScript construction
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core terminal_app`

### Task 8: CREATE `crates/shards-core/src/terminal/registry.rs`

- **ACTION**: CREATE registry for backend lookup
- **IMPLEMENT**:
  ```rust
  use std::collections::HashMap;
  use std::sync::LazyLock;

  use super::backends::{GhosttyBackend, ITermBackend, TerminalAppBackend};
  use super::traits::TerminalBackend;
  use super::types::TerminalType;

  static REGISTRY: LazyLock<TerminalRegistry> = LazyLock::new(TerminalRegistry::new);

  struct TerminalRegistry {
      backends: HashMap<TerminalType, Box<dyn TerminalBackend>>,
  }

  impl TerminalRegistry {
      fn new() -> Self {
          let mut backends: HashMap<TerminalType, Box<dyn TerminalBackend>> = HashMap::new();
          backends.insert(TerminalType::Ghostty, Box::new(GhosttyBackend));
          backends.insert(TerminalType::ITerm, Box::new(ITermBackend));
          backends.insert(TerminalType::TerminalApp, Box::new(TerminalAppBackend));
          Self { backends }
      }

      fn get(&self, terminal_type: &TerminalType) -> Option<&dyn TerminalBackend> {
          self.backends.get(terminal_type).map(|b| b.as_ref())
      }
  }

  pub fn get_backend(terminal_type: &TerminalType) -> Option<&'static dyn TerminalBackend> {
      REGISTRY.get(terminal_type)
  }

  /// Detect available terminal (Ghostty > iTerm > Terminal.app)
  pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
      // Check in preference order
      if get_backend(&TerminalType::Ghostty).map(|b| b.is_available()).unwrap_or(false) {
          return Ok(TerminalType::Ghostty);
      }
      if get_backend(&TerminalType::ITerm).map(|b| b.is_available()).unwrap_or(false) {
          return Ok(TerminalType::ITerm);
      }
      if get_backend(&TerminalType::TerminalApp).map(|b| b.is_available()).unwrap_or(false) {
          return Ok(TerminalType::TerminalApp);
      }
      Err(TerminalError::NoTerminalFound)
  }
  ```
- **MIRROR**: `crates/shards-core/src/agents/registry.rs`
- **GOTCHA**: `Native` type is NOT registered - it delegates to detected type
- **TESTS**: Test get_backend, detect_terminal
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core registry`

### Task 9: UPDATE `crates/shards-core/src/terminal/mod.rs`

- **ACTION**: ADD new module exports
- **IMPLEMENT**: Update from:
  ```rust
  pub mod errors;
  pub mod handler;
  pub mod operations;
  pub mod types;
  ```
  To:
  ```rust
  pub mod backends;
  pub mod common;
  pub mod errors;
  pub mod handler;
  pub mod operations;
  pub mod registry;
  pub mod traits;
  pub mod types;
  ```
- **VALIDATE**: `cargo check -p shards-core`

### Task 10: UPDATE `crates/shards-core/src/terminal/operations.rs`

- **ACTION**: REFACTOR to use registry dispatch
- **IMPLEMENT**:
  1. Remove moved constants (ITERM_SCRIPT, TERMINAL_SCRIPT, etc.)
  2. Remove moved utility functions (shell_escape, applescript_escape, escape_regex, build_cd_command)
  3. Import from common/escape.rs instead
  4. Update `detect_terminal()` to call `registry::detect_terminal()`
  5. Update `build_spawn_command()` to delegate to backends (or deprecate if unused)
  6. Update `execute_spawn_script()` to use registry:
     ```rust
     pub fn execute_spawn_script(
         config: &SpawnConfig,
         window_title: Option<&str>,
     ) -> Result<Option<String>, TerminalError> {
         config.validate()?;

         let terminal_type = match config.terminal_type {
             TerminalType::Native => registry::detect_terminal()?,
             ref t => t.clone(),
         };

         let backend = registry::get_backend(&terminal_type)
             .ok_or(TerminalError::NoTerminalFound)?;

         backend.execute_spawn(config, window_title)
     }
     ```
  7. Update `close_terminal_window()` similarly
  8. Keep `validate_working_directory()` and `extract_command_name()` (utilities)
  9. Keep `app_exists_macos()` (used by backends)
- **GOTCHA**: `Native` handling - detect first, then delegate
- **GOTCHA**: Keep `app_exists_macos` public for backends to use
- **TESTS**: Keep existing tests, update to use new imports
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core operations`

### Task 11: Move `app_exists_macos` to common module

- **ACTION**: Move `app_exists_macos` function to `common/detection.rs`
- **IMPLEMENT**: Create `crates/shards-core/src/terminal/common/detection.rs`:
  ```rust
  /// Check if a macOS application exists
  #[cfg(target_os = "macos")]
  pub fn app_exists_macos(app_name: &str) -> bool {
      // Move implementation from operations.rs:162-179
  }

  #[cfg(not(target_os = "macos"))]
  pub fn app_exists_macos(_app_name: &str) -> bool {
      false
  }
  ```
- **UPDATE**: `common/mod.rs` to export detection module
- **UPDATE**: backends to import from common/detection
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core`

### Task 12: UPDATE `crates/shards-core/src/terminal/handler.rs`

- **ACTION**: Update detection call
- **IMPLEMENT**: Change import from `operations::detect_terminal` to `registry::detect_terminal` where used
- **VERIFY**: Line 121 calls `operations::detect_terminal()` - update to use registry
- **GOTCHA**: Minimal changes - handler should mostly work unchanged
- **VALIDATE**: `cargo check -p shards-core && cargo test -p shards-core handler`

### Task 13: Clean up operations.rs tests

- **ACTION**: Move/update tests to appropriate modules
- **IMPLEMENT**:
  1. Tests for escape functions -> `common/escape.rs`
  2. Tests for backend behavior -> respective backend files
  3. Keep integration tests in operations.rs that test full flow
- **VALIDATE**: `cargo test -p shards-core terminal`

### Task 14: Run full validation

- **ACTION**: Final validation of all changes
- **IMPLEMENT**: Run full test suite and verify no regressions
- **VALIDATE**:
  ```bash
  cargo check -p shards-core
  cargo test -p shards-core
  cargo clippy -p shards-core -- -D warnings
  cargo build -p shards-core
  ```

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `common/escape.rs` | shell_escape, applescript_escape, escape_regex | String escaping |
| `backends/ghostty.rs` | name, is_available, spawn command structure | Ghostty backend |
| `backends/iterm.rs` | name, is_available, AppleScript structure | iTerm backend |
| `backends/terminal_app.rs` | name, is_available, AppleScript structure | Terminal.app backend |
| `registry.rs` | get_backend, detect_terminal | Registry lookup |
| `operations.rs` | Integration: spawn + close via registry | End-to-end flow |

### Edge Cases Checklist

- [ ] `Native` terminal type delegates correctly
- [ ] Unknown terminal type returns error
- [ ] Backend not available returns appropriate error
- [ ] Window ID capture works for all backends
- [ ] Close with None window_id skips (doesn't error)
- [ ] Paths with special characters escape correctly
- [ ] AppleScript strings escape correctly

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check -p shards-core && cargo clippy -p shards-core -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p shards-core terminal
```

**EXPECT**: All terminal module tests pass

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, release build succeeds

---

## Acceptance Criteria

- [ ] Each terminal backend is in its own file (~100 lines each)
- [ ] `TerminalBackend` trait defines the interface
- [ ] No `match TerminalType` outside of registry dispatch
- [ ] All existing tests pass
- [ ] `operations.rs` reduced from 777 to ~100 lines
- [ ] Adding a new terminal only requires new file + registration

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (clippy + check) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] All acceptance criteria met
- [ ] Code mirrors existing agent backend pattern

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Platform-specific code breaks | LOW | HIGH | Keep `#[cfg(target_os)]` guards, test on macOS |
| Test coverage gaps after split | MEDIUM | MEDIUM | Move tests alongside code, verify coverage |
| Circular imports between modules | LOW | MEDIUM | Careful dependency ordering: traits → common → backends → registry |
| `Native` delegation infinite loop | LOW | HIGH | Guard in registry: detect returns non-Native only |

---

## Notes

**Parallel Work Coordination (Issues 52, 53):**
- Issue 52 (sessions/operations.rs split) - No conflicts, different module
- Issue 53 (config module split) - No conflicts, different module
- This refactor is isolated to `terminal/` directory

**Future Work (out of scope for this issue):**
- Add Alacritty, Warp, Kitty, WezTerm backends
- Linux/Windows platform support
- Backend configuration options

**Design Decision: Registry vs Direct Dispatch**
Chose registry pattern (like agents) over direct enum dispatch because:
1. Matches existing codebase pattern
2. Allows future backend registration at runtime
3. Cleaner separation of backend lookup from business logic
