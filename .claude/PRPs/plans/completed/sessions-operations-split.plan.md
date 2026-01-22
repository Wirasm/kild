# Feature: Split sessions/operations.rs into Focused Modules

## Summary

Refactor the 955-line `crates/shards-core/src/sessions/operations.rs` into three focused modules: `validation.rs`, `ports.rs`, and `persistence.rs`. This follows the established codebase pattern (see `health/storage.rs`) of separating concerns into cohesive modules while maintaining backward compatibility through re-exports from `operations.rs`.

## User Story

As a developer working on the shards codebase
I want sessions operations split into focused modules
So that I can navigate and maintain code more easily, with each file having a single responsibility

## Problem Statement

`operations.rs` at 955 lines mixes three unrelated concerns:
1. **Port allocation** - Managing port ranges for sessions (7 functions)
2. **File I/O** - Persisting sessions to disk (6 functions)
3. **Validation** - Input validation for sessions and branches (3 functions)

This makes the file hard to navigate and maintain. Functions with different responsibilities are neighbors, making it difficult to understand or test individual concerns in isolation.

## Solution Statement

Split `operations.rs` into focused modules following the health module's pattern (`health/mod.rs` with separate `storage.rs`):

- `validation.rs` - All validation logic (~150 lines with tests)
- `ports.rs` - Port allocation logic (~200 lines with tests)
- `persistence.rs` - File I/O operations (~300 lines with tests)
- `operations.rs` - Re-exports only for backward compatibility (~30 lines)

## Metadata

| Field            | Value                                      |
| ---------------- | ------------------------------------------ |
| Type             | REFACTOR                                   |
| Complexity       | MEDIUM                                     |
| Systems Affected | sessions module                            |
| Dependencies     | None (internal refactoring)                |
| Estimated Tasks  | 10                                         |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   sessions/                                                                   ║
║   ├── mod.rs              (4 lines)                                           ║
║   ├── types.rs            (283 lines)                                         ║
║   ├── errors.rs           (129 lines)                                         ║
║   ├── handler.rs          (854 lines)                                         ║
║   └── operations.rs       (955 lines) ◄── PROBLEM: 3 concerns mixed           ║
║         ├── validate_session_request()     [validation]                       ║
║         ├── validate_branch_name()         [validation]                       ║
║         ├── validate_session_structure()   [validation]                       ║
║         ├── generate_session_id()          [ports]                            ║
║         ├── calculate_port_range()         [ports]                            ║
║         ├── allocate_port_range()          [ports]                            ║
║         ├── find_next_available_range()    [ports]                            ║
║         ├── is_port_range_available()      [ports]                            ║
║         ├── generate_port_env_vars()       [ports]                            ║
║         ├── ensure_sessions_directory()    [persistence]                      ║
║         ├── save_session_to_file()         [persistence]                      ║
║         ├── load_sessions_from_files()     [persistence]                      ║
║         ├── load_session_from_file()       [persistence]                      ║
║         ├── find_session_by_name()         [persistence]                      ║
║         └── remove_session_file()          [persistence]                      ║
║                                                                               ║
║   PAIN_POINT: Hard to navigate, test, or maintain individual concerns         ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   sessions/                                                                   ║
║   ├── mod.rs              (12 lines - adds pub mod declarations)              ║
║   ├── types.rs            (283 lines - unchanged)                             ║
║   ├── errors.rs           (129 lines - unchanged)                             ║
║   ├── handler.rs          (854 lines - unchanged, imports still work)         ║
║   ├── operations.rs       (~30 lines - re-exports only)                       ║
║   │                       └── pub use validation::*;                          ║
║   │                       └── pub use ports::*;                               ║
║   │                       └── pub use persistence::*;                         ║
║   │                                                                           ║
║   ├── validation.rs       (~150 lines) ◄── NEW: Focused on validation         ║
║   │     ├── validate_session_request()                                        ║
║   │     ├── validate_branch_name()                                            ║
║   │     └── validate_session_structure()                                      ║
║   │                                                                           ║
║   ├── ports.rs            (~200 lines) ◄── NEW: Focused on port allocation    ║
║   │     ├── generate_session_id()                                             ║
║   │     ├── calculate_port_range()                                            ║
║   │     ├── allocate_port_range()                                             ║
║   │     ├── find_next_available_range()                                       ║
║   │     ├── is_port_range_available()                                         ║
║   │     └── generate_port_env_vars()                                          ║
║   │                                                                           ║
║   └── persistence.rs      (~300 lines) ◄── NEW: Focused on file I/O           ║
║         ├── ensure_sessions_directory()                                       ║
║         ├── save_session_to_file()                                            ║
║         ├── load_sessions_from_files()                                        ║
║         ├── load_session_from_file()                                          ║
║         ├── find_session_by_name()                                            ║
║         └── remove_session_file()                                             ║
║                                                                               ║
║   VALUE_ADD: Each file has single responsibility, ~200 lines per file         ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | Developer Impact |
|----------|--------|-------|------------------|
| `handler.rs` imports | `use operations::*` | Same (re-exports) | No changes needed |
| `sessions/mod.rs` | 4 modules declared | 7 modules declared | Module list grows |
| Code navigation | 955-line file | 3 focused files | Know where to look |
| Testing | All tests in one file | Tests per concern | Faster iteration |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards-core/src/sessions/operations.rs` | 1-308 | Functions to EXTRACT |
| P0 | `crates/shards-core/src/sessions/operations.rs` | 309-955 | Tests to MOVE with functions |
| P0 | `crates/shards-core/src/sessions/handler.rs` | 1-50 | Verify import patterns still work |
| P1 | `crates/shards-core/src/health/mod.rs` | 1-12 | Pattern for re-exports |
| P1 | `crates/shards-core/src/health/storage.rs` | 1-100 | Pattern for split module |
| P2 | `crates/shards-core/src/sessions/types.rs` | 1-30 | Types used in functions |
| P2 | `crates/shards-core/src/sessions/errors.rs` | 1-55 | Errors used in functions |

---

## Patterns to Mirror

**MODULE_RE_EXPORT_PATTERN:**
```rust
// SOURCE: crates/shards-core/src/health/mod.rs:1-12
// COPY THIS PATTERN:
pub mod errors;
pub mod handler;
pub mod operations;
pub mod storage;  // additional focused module
pub mod types;

// Re-export commonly used types
pub use errors::HealthError;
pub use handler::{get_health_all_sessions, get_health_single_session};
pub use operations::{get_idle_threshold_minutes, set_idle_threshold_minutes};
pub use storage::{HealthSnapshot, load_history, save_snapshot};
```

**FOCUSED_MODULE_IMPORTS:**
```rust
// SOURCE: crates/shards-core/src/health/storage.rs:1-10
// COPY THIS PATTERN:
//! Historical health metrics storage
//!
//! Stores health snapshots over time for trend analysis.

use crate::health::types::HealthOutput;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::warn;
```

**VALIDATION_IMPORTS:**
```rust
// SOURCE: crates/shards-core/src/sessions/operations.rs:1-4
// COPY THIS PATTERN for validation.rs:
use crate::sessions::{errors::SessionError, types::*};
```

**PERSISTENCE_IMPORTS:**
```rust
// SOURCE: crates/shards-core/src/sessions/operations.rs:1-4
// COPY THIS PATTERN for persistence.rs:
use crate::sessions::{errors::SessionError, types::*};
use std::fs;
use std::path::Path;
use tracing::warn;
```

**PORTS_IMPORTS:**
```rust
// SOURCE: crates/shards-core/src/sessions/operations.rs:1-4
// COPY THIS PATTERN for ports.rs:
use crate::sessions::{errors::SessionError, types::*};
use std::path::Path;
```

**TEST_STRUCTURE:**
```rust
// SOURCE: crates/shards-core/src/sessions/operations.rs:309-320
// COPY THIS PATTERN:
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_session_request_success() {
        let result = validate_session_request("test", "echo hello", "claude");
        assert!(result.is_ok());
        // ...
    }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards-core/src/sessions/validation.rs` | CREATE | Extract validation functions |
| `crates/shards-core/src/sessions/ports.rs` | CREATE | Extract port allocation functions |
| `crates/shards-core/src/sessions/persistence.rs` | CREATE | Extract file I/O functions |
| `crates/shards-core/src/sessions/operations.rs` | UPDATE | Replace with re-exports |
| `crates/shards-core/src/sessions/mod.rs` | UPDATE | Add new module declarations |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Public API changes** - All existing `operations::*` imports must continue to work
- **New functionality** - This is purely a refactor, no new features
- **Handler changes** - `handler.rs` should not need any modifications
- **Error changes** - `errors.rs` remains unchanged
- **Type changes** - `types.rs` remains unchanged
- **Documentation additions** - No new doc comments beyond module-level docs
- **Performance optimizations** - Maintain exact same behavior

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `crates/shards-core/src/sessions/validation.rs`

- **ACTION**: CREATE new file with validation functions extracted from operations.rs
- **IMPLEMENT**: Move these functions with their tests:
  - `validate_session_request()` (lines 6-24)
  - `validate_branch_name()` (lines 125-138)
  - `validate_session_structure()` (lines 249-276) - keep as `pub(crate)` or `pub(super)`
  - Related tests: `test_validate_session_request_*`, `test_validate_branch_name`, `test_validate_session_structure`
- **MIRROR**: `crates/shards-core/src/health/storage.rs:1-10` for imports pattern
- **IMPORTS**:
  ```rust
  //! Session input validation
  //!
  //! Validates session requests, branch names, and session structure.

  use crate::sessions::{errors::SessionError, types::*};
  ```
- **GOTCHA**: `validate_session_structure` returns `Result<(), String>` not `SessionError` - keep signature
- **VALIDATE**: `cargo check -p shards-core`

### Task 2: CREATE `crates/shards-core/src/sessions/ports.rs`

- **ACTION**: CREATE new file with port allocation functions extracted from operations.rs
- **IMPLEMENT**: Move these functions with their tests:
  - `generate_session_id()` (lines 26-28)
  - `calculate_port_range()` (lines 30-33)
  - `allocate_port_range()` (lines 35-47) - DEPENDS on persistence::load_sessions_from_files
  - `find_next_available_range()` (lines 49-92)
  - `is_port_range_available()` (lines 94-106)
  - `generate_port_env_vars()` (lines 108-123)
  - Related tests: `test_generate_session_id`, `test_calculate_port_range`
- **MIRROR**: `crates/shards-core/src/health/storage.rs:1-10` for imports pattern
- **IMPORTS**:
  ```rust
  //! Port allocation and management
  //!
  //! Manages port range allocation for sessions to avoid conflicts.

  use crate::sessions::{errors::SessionError, types::*};
  use std::path::Path;
  ```
- **GOTCHA**: `allocate_port_range` calls `load_sessions_from_files` - use `super::persistence::load_sessions_from_files` or import from crate path
- **VALIDATE**: `cargo check -p shards-core`

### Task 3: CREATE `crates/shards-core/src/sessions/persistence.rs`

- **ACTION**: CREATE new file with file I/O functions extracted from operations.rs
- **IMPLEMENT**: Move these functions with their tests:
  - `ensure_sessions_directory()` (lines 140-143)
  - `save_session_to_file()` (lines 145-170)
  - `load_sessions_from_files()` (lines 172-237)
  - `load_session_from_file()` (lines 239-247)
  - `find_session_by_name()` (lines 278-292)
  - `remove_session_file()` (lines 294-307)
  - Related tests: `test_ensure_sessions_directory`, `test_save_session_*`, `test_load_sessions_*`, `test_find_session_by_name`, `test_remove_session_file`
- **MIRROR**: `crates/shards-core/src/health/storage.rs:1-10` for imports pattern
- **IMPORTS**:
  ```rust
  //! Session file persistence
  //!
  //! Handles reading/writing session data to disk with atomic operations.

  use crate::sessions::{errors::SessionError, types::*};
  use std::fs;
  use std::path::Path;
  use tracing::warn;
  ```
- **GOTCHA**: `load_session_from_file` calls `find_session_by_name` - both in same file, OK
- **GOTCHA**: `load_sessions_from_files` uses `super::validation::validate_session_structure` - ensure import works
- **VALIDATE**: `cargo check -p shards-core`

### Task 4: UPDATE `crates/shards-core/src/sessions/operations.rs`

- **ACTION**: REPLACE entire file content with re-exports only
- **IMPLEMENT**:
  ```rust
  //! Session operations re-exports
  //!
  //! This module re-exports from focused submodules for backward compatibility.
  //! Direct imports: `use crate::sessions::operations::*`
  //!
  //! For new code, consider importing from specific modules:
  //! - `crate::sessions::validation` - Input validation
  //! - `crate::sessions::ports` - Port allocation
  //! - `crate::sessions::persistence` - File I/O

  pub use super::persistence::*;
  pub use super::ports::*;
  pub use super::validation::*;
  ```
- **MIRROR**: None - unique pattern for backward compatibility
- **GOTCHA**: Tests are moved to respective modules, not kept here
- **VALIDATE**: `cargo check -p shards-core`

### Task 5: UPDATE `crates/shards-core/src/sessions/mod.rs`

- **ACTION**: ADD new module declarations
- **IMPLEMENT**: Update to include new modules:
  ```rust
  pub mod errors;
  pub mod handler;
  pub mod operations;
  pub mod persistence;
  pub mod ports;
  pub mod types;
  pub mod validation;
  ```
- **MIRROR**: `crates/shards-core/src/health/mod.rs:1-5` for module declaration pattern
- **GOTCHA**: Order alphabetically or by dependency - alphabetical is simpler
- **VALIDATE**: `cargo check -p shards-core`

### Task 6: FIX cross-module imports in `ports.rs`

- **ACTION**: UPDATE `allocate_port_range` to import from `persistence`
- **IMPLEMENT**: Ensure `allocate_port_range` can call `load_sessions_from_files`:
  ```rust
  pub fn allocate_port_range(
      sessions_dir: &Path,
      port_count: u16,
      base_port: u16,
  ) -> Result<(u16, u16), SessionError> {
      let (existing_sessions, _) = super::persistence::load_sessions_from_files(sessions_dir)?;
      // OR: use crate::sessions::persistence::load_sessions_from_files(sessions_dir)?;

      let (start_port, end_port) =
          find_next_available_range(&existing_sessions, port_count, base_port)?;

      Ok((start_port, end_port))
  }
  ```
- **MIRROR**: None - standard Rust cross-module import
- **GOTCHA**: Choose `super::persistence::` or `crate::sessions::persistence::` - prefer `super::` for locality
- **VALIDATE**: `cargo check -p shards-core`

### Task 7: FIX cross-module imports in `persistence.rs`

- **ACTION**: UPDATE `load_sessions_from_files` to import `validate_session_structure` from `validation`
- **IMPLEMENT**: Ensure validation import works:
  ```rust
  use super::validation::validate_session_structure;
  // OR at call site:
  match super::validation::validate_session_structure(&session) {
  ```
- **MIRROR**: None - standard Rust cross-module import
- **GOTCHA**: `validate_session_structure` may need to be `pub(crate)` not `pub` if only used internally
- **VALIDATE**: `cargo check -p shards-core`

### Task 8: RUN full type check and lint

- **ACTION**: VERIFY all code compiles and passes lint
- **IMPLEMENT**:
  ```bash
  cargo check -p shards-core
  cargo clippy -p shards-core -- -D warnings
  ```
- **MIRROR**: None
- **GOTCHA**: Fix any clippy warnings about unused imports or dead code
- **VALIDATE**: Exit 0 for both commands

### Task 9: RUN all tests

- **ACTION**: VERIFY all tests pass after refactoring
- **IMPLEMENT**:
  ```bash
  cargo test -p shards-core
  ```
- **MIRROR**: None
- **GOTCHA**: Tests should find functions via re-exports - if not, fix imports in test modules
- **VALIDATE**: All tests pass (expect ~30+ tests in sessions module)

### Task 10: VERIFY handler imports still work

- **ACTION**: VERIFY `handler.rs` compiles without changes
- **IMPLEMENT**: Check that `handler.rs` line 7 still works:
  ```rust
  use crate::sessions::{errors::SessionError, operations, types::*};
  ```
  And all usages like `operations::validate_session_request()` work.
- **MIRROR**: None
- **GOTCHA**: If handler tests fail, the re-exports are broken
- **VALIDATE**: `cargo test -p shards-core sessions::handler`

---

## Testing Strategy

### Unit Tests to Move

| Original Location | New Location | Test Cases |
|-------------------|--------------|------------|
| `operations.rs:314-344` | `validation.rs` | `test_validate_session_request_*` (4 tests) |
| `operations.rs:359-368` | `validation.rs` | `test_validate_branch_name` |
| `operations.rs:843-953` | `validation.rs` | `test_validate_session_structure` |
| `operations.rs:347-357` | `ports.rs` | `test_generate_session_id`, `test_calculate_port_range` |
| `operations.rs:370-438` | `persistence.rs` | `test_ensure_sessions_directory`, `test_save_session_to_file` |
| `operations.rs:440-595` | `persistence.rs` | `test_save_session_atomic_*` (3 tests) |
| `operations.rs:597-673` | `persistence.rs` | `test_load_sessions_*` (2 tests) |
| `operations.rs:687-733` | `persistence.rs` | `test_find_session_by_name` |
| `operations.rs:735-783` | `persistence.rs` | `test_remove_session_file` |
| `operations.rs:786-840` | `persistence.rs` | `test_load_sessions_with_invalid_files` |

### Edge Cases Checklist

- [x] Re-exports work for all public functions
- [x] `handler.rs` compiles without changes
- [x] Cross-module calls work (ports -> persistence, persistence -> validation)
- [x] All original tests pass in new locations
- [x] No orphaned imports or dead code

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check -p shards-core && cargo clippy -p shards-core -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p shards-core
```

**EXPECT**: All tests pass (28+ tests in sessions module)

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, release build succeeds

---

## Acceptance Criteria

- [x] `validation.rs` contains all validation logic with tests
- [x] `ports.rs` contains all port allocation logic with tests
- [x] `persistence.rs` contains all file I/O logic with tests
- [x] `operations.rs` under 50 lines (re-exports only)
- [x] Each new file under 350 lines (including tests)
- [x] All existing tests pass
- [x] No public API changes - `operations::*` imports work unchanged
- [x] `handler.rs` requires no modifications

---

## Completion Checklist

- [ ] Task 1: `validation.rs` created with functions and tests
- [ ] Task 2: `ports.rs` created with functions and tests
- [ ] Task 3: `persistence.rs` created with functions and tests
- [ ] Task 4: `operations.rs` reduced to re-exports
- [ ] Task 5: `mod.rs` updated with new module declarations
- [ ] Task 6: Cross-module imports fixed in `ports.rs`
- [ ] Task 7: Cross-module imports fixed in `persistence.rs`
- [ ] Task 8: `cargo check` and `cargo clippy` pass
- [ ] Task 9: All tests pass
- [ ] Task 10: Handler imports verified

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Circular dependency between modules | LOW | HIGH | Validation has no deps, persistence uses validation, ports uses persistence - natural layering |
| Test isolation breaks | LOW | MEDIUM | Each test file has same `#[cfg(test)] mod tests` pattern |
| Import path confusion | MEDIUM | LOW | Use `super::` consistently for sibling module access |
| Missed function during extraction | LOW | HIGH | Line-by-line verification against operations.rs |

---

## Notes

**Parallel Work Context**: This refactor is being done in parallel with:
- Issue #50: Terminal backends extraction
- Issue #53: Config module reorganization

These issues are independent and should not conflict, as they touch different modules.

**Function Dependencies Analysis**:

```
validation.rs:
  validate_session_request() - PURE, no deps
  validate_branch_name() - PURE, no deps
  validate_session_structure() - PURE, no deps

ports.rs:
  generate_session_id() - PURE, no deps
  calculate_port_range() - PURE, no deps
  find_next_available_range() - PURE, takes sessions slice
  is_port_range_available() - PURE, takes sessions slice
  generate_port_env_vars() - PURE, takes session ref
  allocate_port_range() - CALLS persistence::load_sessions_from_files

persistence.rs:
  ensure_sessions_directory() - fs::create_dir_all
  save_session_to_file() - fs::write, serde_json
  load_sessions_from_files() - fs::read, serde_json, CALLS validation::validate_session_structure
  load_session_from_file() - CALLS find_session_by_name
  find_session_by_name() - CALLS load_sessions_from_files
  remove_session_file() - fs::remove_file
```

**Line Count Estimates**:
- `validation.rs`: ~50 lines functions + ~100 lines tests = ~150 lines
- `ports.rs`: ~80 lines functions + ~50 lines tests = ~130 lines
- `persistence.rs`: ~100 lines functions + ~200 lines tests = ~300 lines
- `operations.rs`: ~30 lines (re-exports + doc comment)
