# Feature: Cargo Workspace Restructure

## Summary

Transform the single-package Shards codebase into a Cargo workspace with three crates: `shards-core` (shared library), `shards` (CLI binary), and `shards-ui` (optional UI binary). This enables independent distribution of CLI-only or UI-only binaries, clear separation of concerns, and parallel development of CLI and UI features. The core library contains all business logic (sessions, git, terminal, process, health, cleanup, config) while presentation layers (CLI, UI) only handle user interaction.

## User Story

As a developer/distributor of Shards
I want the codebase split into core/cli/ui packages
So that I can distribute CLI-only, UI-only, or full bundles independently

## Problem Statement

The current single-package architecture:
1. Cannot distribute CLI without UI dependencies (once UI is added)
2. Cannot distribute UI without CLI code
3. Has no clear separation between business logic and presentation
4. Makes parallel development of CLI and UI features prone to conflicts
5. Has no defined public API surface for the core logic

## Solution Statement

Create a Cargo virtual workspace with three member crates:
- **shards-core**: Library crate containing all business logic
- **shards**: Binary crate for CLI (depends on shards-core)
- **shards-ui**: Binary crate for GUI (depends on shards-core, feature-gated GPUI)

Both binaries share the same core logic, session storage (`~/.shards/`), and configuration hierarchy.

## Metadata

| Field            | Value                                          |
| ---------------- | ---------------------------------------------- |
| Type             | REFACTOR                                       |
| Complexity       | HIGH                                           |
| Systems Affected | build system, all modules, tests, CI           |
| Dependencies     | None new (workspace reorganization only)       |
| Estimated Tasks  | 12                                             |

---

## Architecture Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   shards/                    Single package, everything mixed                 ║
║   ├── Cargo.toml            [package] name = "shards"                        ║
║   ├── src/                                                                    ║
║   │   ├── main.rs           Entry point (CLI)                                ║
║   │   ├── lib.rs            Exports everything (CLI + core)                  ║
║   │   ├── cli/              CLI-specific code                                ║
║   │   │   ├── app.rs        Clap definitions                                 ║
║   │   │   ├── commands.rs   Command handlers                                 ║
║   │   │   └── table.rs      Table formatting                                 ║
║   │   ├── core/             Config, logging, errors                          ║
║   │   ├── sessions/         Session management                               ║
║   │   ├── terminal/         Terminal spawning                                ║
║   │   ├── git/              Git/worktree operations                          ║
║   │   ├── process/          Process monitoring                               ║
║   │   ├── health/           Health metrics                                   ║
║   │   ├── cleanup/          Resource cleanup                                 ║
║   │   └── files/            File operations                                  ║
║   └── tests/                All tests in one place                           ║
║                                                                               ║
║   PAIN POINTS:                                                                ║
║   - Cannot distribute CLI without all code                                   ║
║   - No place for UI to live without pulling CLI                              ║
║   - lib.rs exports CLI-specific functions (build_cli, run_command)           ║
║   - No clear public API for core functionality                               ║
║   - All deps in one Cargo.toml (clap pulled even if only using core)         ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   shards/                    Virtual workspace root                           ║
║   ├── Cargo.toml            [workspace] members = ["crates/*"]               ║
║   ├── Cargo.lock            Shared lockfile                                  ║
║   │                                                                           ║
║   └── crates/                                                                 ║
║       ├── shards-core/      Library crate (business logic)                   ║
║       │   ├── Cargo.toml    [package] name = "shards-core"                   ║
║       │   └── src/                                                            ║
║       │       ├── lib.rs    Public API exports                               ║
║       │       ├── config/   Config types & loading                           ║
║       │       ├── errors/   Error types & traits                             ║
║       │       ├── sessions/ Session management                               ║
║       │       ├── terminal/ Terminal spawning                                ║
║       │       ├── git/      Git/worktree operations                          ║
║       │       ├── process/  Process monitoring                               ║
║       │       ├── health/   Health metrics                                   ║
║       │       ├── cleanup/  Resource cleanup                                 ║
║       │       ├── files/    File operations                                  ║
║       │       └── logging/  Logging setup                                    ║
║       │                                                                       ║
║       ├── shards/           Binary crate (CLI)                               ║
║       │   ├── Cargo.toml    depends on shards-core, clap                     ║
║       │   └── src/                                                            ║
║       │       ├── main.rs   CLI entry point                                  ║
║       │       ├── app.rs    Clap definitions                                 ║
║       │       ├── commands.rs Command handlers                               ║
║       │       └── table.rs  Table formatting                                 ║
║       │                                                                       ║
║       └── shards-ui/        Binary crate (GUI) - scaffolding only            ║
║           ├── Cargo.toml    depends on shards-core, gpui (optional)          ║
║           └── src/                                                            ║
║               └── main.rs   Placeholder for Phase 1 of UI PRD                ║
║                                                                               ║
║   DISTRIBUTION OPTIONS:                                                       ║
║   - cargo install shards        → CLI only                                   ║
║   - cargo install shards-ui     → UI only                                    ║
║   - Both: CLI + UI share ~/.shards/ data                                     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Dependency Flow

```
                    ┌─────────────────────┐
                    │    shards-core      │
                    │    (library)        │
                    │                     │
                    │ - sessions::*       │
                    │ - terminal::*       │
                    │ - git::*            │
                    │ - process::*        │
                    │ - health::*         │
                    │ - cleanup::*        │
                    │ - config::*         │
                    │ - files::*          │
                    └─────────┬───────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
              ▼               │               ▼
    ┌─────────────────┐       │     ┌─────────────────┐
    │     shards      │       │     │   shards-ui     │
    │     (CLI)       │       │     │    (GUI)        │
    │                 │       │     │                 │
    │ + clap          │       │     │ + gpui          │
    │ + app.rs        │       │     │ + views/        │
    │ + commands.rs   │       │     │ + state/        │
    │ + table.rs      │       │     │                 │
    └─────────────────┘       │     └─────────────────┘
              │               │               │
              └───────────────┼───────────────┘
                              │
                              ▼
                    ┌─────────────────────┐
                    │  ~/.shards/         │
                    │  (shared data)      │
                    │                     │
                    │ - sessions/*.json   │
                    │ - config.toml       │
                    │ - .worktrees/       │
                    └─────────────────────┘
```

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `Cargo.toml` | all | Current dependencies to redistribute |
| P0 | `src/lib.rs` | all | Current exports to understand public API |
| P0 | `src/main.rs` | all | Current entry point pattern |
| P0 | `src/cli/commands.rs` | 1-50 | How CLI calls into core modules |
| P1 | `src/sessions/handler.rs` | all | Primary API for session operations |
| P1 | `src/sessions/types.rs` | all | Core types that must be exported |
| P1 | `src/core/config.rs` | 1-100 | Config types and loading |
| P1 | `src/terminal/handler.rs` | all | Terminal API surface |
| P2 | `src/health/handler.rs` | all | Health API surface |
| P2 | `src/cleanup/handler.rs` | all | Cleanup API surface |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [Cargo Workspaces](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) | Full chapter | Workspace setup syntax |
| [Cargo Reference - Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) | workspace.dependencies | Shared dependency inheritance |
| [Cargo Reference - Features](https://doc.rust-lang.org/cargo/reference/features.html) | Optional dependencies | For UI feature gating |

---

## Patterns to Mirror

**MODULE_EXPORTS (from lib.rs):**
```rust
// SOURCE: src/lib.rs:1-14
// CURRENT PATTERN (exports CLI-specific):
pub mod cleanup;
pub mod cli;
pub mod core;
// ...
pub use cli::app::build_cli;
pub use cli::commands::run_command;
pub use core::logging::init_logging;

// NEW PATTERN for shards-core/src/lib.rs:
// Only export core modules, not CLI
pub mod cleanup;
pub mod config;
pub mod errors;
pub mod files;
pub mod git;
pub mod health;
pub mod logging;
pub mod process;
pub mod sessions;
pub mod terminal;

// Re-export commonly used types at crate root
pub use config::ShardsConfig;
pub use sessions::{Session, SessionStatus, CreateSessionRequest};
pub use sessions::handler as sessions;
pub use terminal::handler as terminal;
pub use health::handler as health;
pub use cleanup::handler as cleanup;
```

**HANDLER_PATTERN (public API):**
```rust
// SOURCE: src/sessions/handler.rs:9-30
// This is the PUBLIC API that both CLI and UI will call
pub fn create_session(
    request: CreateSessionRequest,
    shards_config: &ShardsConfig,
) -> Result<Session, SessionError> {
    // ... implementation
}

pub fn list_sessions() -> Result<Vec<Session>, SessionError> {
    // ... implementation
}

pub fn get_session(branch: &str) -> Result<Session, SessionError> {
    // ... implementation
}

pub fn destroy_session(branch: &str) -> Result<(), SessionError> {
    // ... implementation
}
```

**CLI_CALLS_CORE (command handlers):**
```rust
// SOURCE: src/cli/commands.rs:72-101
// CLI command handler calls into core
fn handle_create_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("...")?;
    let config = ShardsConfig::load_hierarchy().unwrap_or_default();

    // CLI calls core session handler
    match session_handler::create_session(request, &config) {
        Ok(session) => {
            // CLI-specific output formatting
            println!("✅ Shard created successfully!");
            // ...
        }
        Err(e) => {
            eprintln!("❌ Failed to create shard: {}", e);
            // ...
        }
    }
}
```

---

## Files to Change

### Phase A: Workspace Setup

| File | Action | Justification |
|------|--------|---------------|
| `Cargo.toml` | REWRITE | Convert to workspace root with [workspace] section |
| `crates/shards-core/Cargo.toml` | CREATE | Library package manifest |
| `crates/shards/Cargo.toml` | CREATE | CLI binary package manifest |
| `crates/shards-ui/Cargo.toml` | CREATE | UI binary package manifest (scaffold) |

### Phase B: Move Core Modules

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards-core/src/lib.rs` | CREATE | Core library public exports |
| `src/sessions/*` | MOVE | → `crates/shards-core/src/sessions/` |
| `src/terminal/*` | MOVE | → `crates/shards-core/src/terminal/` |
| `src/git/*` | MOVE | → `crates/shards-core/src/git/` |
| `src/process/*` | MOVE | → `crates/shards-core/src/process/` |
| `src/health/*` | MOVE | → `crates/shards-core/src/health/` |
| `src/cleanup/*` | MOVE | → `crates/shards-core/src/cleanup/` |
| `src/files/*` | MOVE | → `crates/shards-core/src/files/` |
| `src/core/config.rs` | MOVE | → `crates/shards-core/src/config/` (restructure) |
| `src/core/errors.rs` | MOVE | → `crates/shards-core/src/errors/` |
| `src/core/logging.rs` | MOVE | → `crates/shards-core/src/logging/` |
| `src/core/events.rs` | MOVE | → `crates/shards-core/src/events/` |

### Phase C: Move CLI

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards/src/main.rs` | CREATE | CLI entry point |
| `src/cli/app.rs` | MOVE | → `crates/shards/src/app.rs` |
| `src/cli/commands.rs` | MOVE | → `crates/shards/src/commands.rs` |
| `src/cli/table.rs` | MOVE | → `crates/shards/src/table.rs` |

### Phase D: Cleanup

| File | Action | Justification |
|------|--------|---------------|
| `src/` | DELETE | Old source directory (after verification) |
| `src/lib.rs` | DELETE | Replaced by crates/shards-core/src/lib.rs |
| `src/main.rs` | DELETE | Replaced by crates/shards/src/main.rs |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **UI implementation** - Only scaffold `shards-ui` with placeholder main.rs (Phase 1 of UI PRD)
- **New features** - No CLI features from cli-core-features.prd.md
- **Refactoring issues #50-53** - Those come AFTER this restructure
- **Tests rewrite** - Tests move with modules, minimal changes
- **CI/CD changes** - Separate task after workspace is working
- **Documentation updates** - README/CLAUDE.md updates are follow-up

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE workspace root `Cargo.toml`

- **ACTION**: Rewrite root Cargo.toml as workspace manifest
- **IMPLEMENT**:
```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/Wirasm/SHARDS"

[workspace.dependencies]
# Core dependencies (shared across crates)
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
dirs = "5.0"
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sysinfo = "0.37.2"
toml = "0.8"
git2 = "0.18"
ignore = "0.4"
glob = "0.3"
walkdir = "2"
tempfile = "3"
uuid = { version = "1", features = ["v4"] }

# CLI-only dependencies
clap = { version = "4.0", features = ["derive"] }

# Internal workspace dependencies
shards-core = { path = "crates/shards-core" }
```
- **GOTCHA**: Preserve edition = "2024" for resolver = "3" compatibility
- **VALIDATE**: `cargo check` should fail (no crates exist yet)

### Task 2: CREATE `crates/shards-core/Cargo.toml`

- **ACTION**: Create library package manifest
- **IMPLEMENT**:
```toml
[package]
name = "shards-core"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Core library for Shards - parallel AI agent worktree management"

[dependencies]
thiserror.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
dirs.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
sysinfo.workspace = true
toml.workspace = true
git2.workspace = true
ignore.workspace = true
glob.workspace = true
walkdir.workspace = true
tempfile.workspace = true
uuid.workspace = true
```
- **MIRROR**: Workspace dependency inheritance pattern
- **VALIDATE**: File exists, valid TOML syntax

### Task 3: CREATE `crates/shards/Cargo.toml`

- **ACTION**: Create CLI binary package manifest
- **IMPLEMENT**:
```toml
[package]
name = "shards"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "CLI for Shards - manage parallel AI development agents"

[[bin]]
name = "shards"
path = "src/main.rs"

[dependencies]
shards-core.workspace = true
clap.workspace = true
tracing.workspace = true
serde_json.workspace = true
```
- **GOTCHA**: Binary name must match package name for `cargo install` to work
- **VALIDATE**: File exists, valid TOML syntax

### Task 4: CREATE `crates/shards-ui/Cargo.toml`

- **ACTION**: Create UI binary package manifest (scaffold only)
- **IMPLEMENT**:
```toml
[package]
name = "shards-ui"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "GUI for Shards - visual shard management dashboard"

[[bin]]
name = "shards-ui"
path = "src/main.rs"

[dependencies]
shards-core.workspace = true

# UI dependencies will be added in Phase 1 of UI PRD
# gpui = { version = "0.2", optional = true }
```
- **VALIDATE**: File exists, valid TOML syntax

### Task 5: MOVE core modules to `crates/shards-core/src/`

- **ACTION**: Move all non-CLI modules
- **IMPLEMENT**:
```bash
# Create directory structure
mkdir -p crates/shards-core/src/{sessions,terminal,git,process,health,cleanup,files,config,errors,logging,events}

# Move modules (preserving git history with git mv)
git mv src/sessions/* crates/shards-core/src/sessions/
git mv src/terminal/* crates/shards-core/src/terminal/
git mv src/git/* crates/shards-core/src/git/
git mv src/process/* crates/shards-core/src/process/
git mv src/health/* crates/shards-core/src/health/
git mv src/cleanup/* crates/shards-core/src/cleanup/
git mv src/files/* crates/shards-core/src/files/

# Move core submodules (restructure from core/ to separate top-level modules)
git mv src/core/config.rs crates/shards-core/src/config/mod.rs
git mv src/core/errors.rs crates/shards-core/src/errors/mod.rs
git mv src/core/logging.rs crates/shards-core/src/logging/mod.rs
git mv src/core/events.rs crates/shards-core/src/events/mod.rs
```
- **GOTCHA**: Use `git mv` to preserve history, not `mv`
- **VALIDATE**: All files moved, `ls crates/shards-core/src/` shows all modules

### Task 6: CREATE `crates/shards-core/src/lib.rs`

- **ACTION**: Create core library public API
- **IMPLEMENT**:
```rust
//! shards-core: Core library for parallel AI agent worktree management
//!
//! This library provides the business logic for managing shards (isolated
//! git worktrees with AI agents). It is used by both the CLI and UI.
//!
//! # Main Entry Points
//!
//! - [`sessions`] - Create, list, destroy, restart sessions
//! - [`health`] - Monitor shard health and metrics
//! - [`cleanup`] - Clean up orphaned resources
//! - [`config`] - Configuration management

pub mod cleanup;
pub mod config;
pub mod errors;
pub mod events;
pub mod files;
pub mod git;
pub mod health;
pub mod logging;
pub mod process;
pub mod sessions;
pub mod terminal;

// Re-export commonly used types at crate root for convenience
pub use config::ShardsConfig;
pub use sessions::types::{CreateSessionRequest, Session, SessionStatus};

// Re-export handler modules as the primary API
pub use cleanup::handler as cleanup_ops;
pub use health::handler as health_ops;
pub use sessions::handler as session_ops;
pub use terminal::handler as terminal_ops;

// Re-export logging initialization
pub use logging::init_logging;
```
- **MIRROR**: Follows lib.rs pattern but excludes CLI-specific exports
- **VALIDATE**: `cargo check -p shards-core` passes

### Task 7: UPDATE import paths in core modules

- **ACTION**: Fix `crate::` references that now point to shards-core
- **IMPLEMENT**: Update all files in `crates/shards-core/src/` to use new paths
  - `crate::core::config` → `crate::config`
  - `crate::core::errors` → `crate::errors`
  - `crate::core::events` → `crate::events`
  - `crate::core::logging` → `crate::logging`
- **GOTCHA**: Use search-replace carefully, some paths stay the same (e.g., `crate::sessions`)
- **VALIDATE**: `cargo check -p shards-core` passes

### Task 8: MOVE CLI to `crates/shards/src/`

- **ACTION**: Move CLI-specific code
- **IMPLEMENT**:
```bash
mkdir -p crates/shards/src

# Move CLI modules
git mv src/cli/app.rs crates/shards/src/app.rs
git mv src/cli/commands.rs crates/shards/src/commands.rs
git mv src/cli/table.rs crates/shards/src/table.rs
```
- **VALIDATE**: Files moved successfully

### Task 9: CREATE `crates/shards/src/main.rs`

- **ACTION**: Create new CLI entry point
- **IMPLEMENT**:
```rust
use shards_core::init_logging;

mod app;
mod commands;
mod table;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let app = app::build_cli();
    let matches = app.get_matches();

    commands::run_command(&matches)?;

    Ok(())
}
```
- **MIRROR**: Original src/main.rs pattern
- **VALIDATE**: File exists with correct content

### Task 10: UPDATE CLI imports to use shards-core

- **ACTION**: Fix imports in CLI crate to reference shards-core
- **IMPLEMENT**: In `crates/shards/src/commands.rs`:
```rust
// OLD:
use crate::cleanup;
use crate::cli::table::truncate;
use crate::core::events;
use crate::core::config::ShardsConfig;
use crate::health;
use crate::process;
use crate::sessions::{handler as session_handler, types::CreateSessionRequest};

// NEW:
use shards_core::{
    cleanup_ops as cleanup,
    events,
    health_ops as health,
    process,
    session_ops as session_handler,
    CreateSessionRequest,
    ShardsConfig,
};
use crate::table::truncate;
```
- **GOTCHA**: `crate::cli::table` becomes `crate::table` (CLI modules are now at crate root)
- **VALIDATE**: `cargo check -p shards` passes

### Task 11: CREATE `crates/shards-ui/src/main.rs`

- **ACTION**: Create placeholder UI entry point
- **IMPLEMENT**:
```rust
//! shards-ui: GUI for Shards
//!
//! This is a placeholder for the GPUI-based UI.
//! See .claude/PRPs/prds/gpui-native-terminal-ui.prd.md for implementation plan.

fn main() {
    eprintln!("shards-ui is not yet implemented.");
    eprintln!("See Phase 1 of gpui-native-terminal-ui.prd.md to begin implementation.");
    std::process::exit(1);
}
```
- **GOTCHA**: Don't add GPUI dependency yet - that's Phase 1 of UI PRD
- **VALIDATE**: `cargo check -p shards-ui` passes

### Task 12: DELETE old source directory and verify

- **ACTION**: Remove old src/ directory, run full validation
- **IMPLEMENT**:
```bash
# Verify nothing references old paths
grep -r "src/cli" . --include="*.rs" || echo "No old CLI refs"
grep -r "src/core" . --include="*.rs" || echo "No old core refs"

# Remove old directories
rm -rf src/
```
- **VALIDATE**:
  - `cargo build` succeeds
  - `cargo test` passes
  - `cargo run -p shards -- list` works
  - `./target/debug/shards --help` works

---

## Testing Strategy

### Validation During Migration

| Checkpoint | Command | Expected Result |
|------------|---------|-----------------|
| After Task 1 | `cargo check` | Fails (no crates) |
| After Task 6 | `cargo check -p shards-core` | Passes |
| After Task 10 | `cargo check -p shards` | Passes |
| After Task 11 | `cargo check -p shards-ui` | Passes |
| After Task 12 | `cargo build` | All crates build |
| After Task 12 | `cargo test` | All tests pass |

### Functional Tests

```bash
# After full migration, verify CLI still works
cargo run -p shards -- --help
cargo run -p shards -- list
cargo run -p shards -- health

# Verify binary name
cargo build -p shards
./target/debug/shards --version
```

### Edge Cases Checklist

- [ ] Existing sessions in ~/.shards/sessions/ still load
- [ ] Config hierarchy still works (project > user > defaults)
- [ ] Terminal spawning works (iTerm, Ghostty, Terminal.app)
- [ ] Process detection finds agents
- [ ] Health monitoring shows correct status
- [ ] Cleanup identifies orphaned resources

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check && cargo clippy --all-targets
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test --workspace
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --workspace && cargo build --workspace
```

**EXPECT**: All tests pass, all crates build

### Level 4: FUNCTIONAL_VALIDATION

```bash
# Test CLI works
cargo run -p shards -- list
cargo run -p shards -- health

# Test binary installation works
cargo install --path crates/shards
shards --version
```

**EXPECT**: Commands work correctly

---

## Acceptance Criteria

- [ ] Workspace has three crates: shards-core, shards, shards-ui
- [ ] `cargo build -p shards-core` builds library
- [ ] `cargo build -p shards` builds CLI binary
- [ ] `cargo build -p shards-ui` builds (placeholder) UI binary
- [ ] `cargo run -p shards -- list` works
- [ ] `cargo test --workspace` passes all tests
- [ ] Existing ~/.shards/ data works with new CLI
- [ ] shards-core has no CLI dependencies (no clap)
- [ ] shards (CLI) depends only on shards-core + clap
- [ ] Git history preserved for moved files

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full build succeeds
- [ ] Level 4: Functional validation passes
- [ ] All acceptance criteria met
- [ ] No references to old src/ directory remain

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Import path breakage | HIGH | HIGH | Fix incrementally, validate at each step |
| Test failures after move | MEDIUM | MEDIUM | Tests move with modules, minimal changes needed |
| Git history loss | LOW | MEDIUM | Use `git mv` for all moves |
| Circular dependencies | LOW | HIGH | Core has no deps on CLI/UI by design |
| CI breaks | MEDIUM | LOW | Separate task to update CI after workspace works |

---

## Post-Restructure Follow-up

After this restructure is complete and merged:

1. **Update CI/CD** - Adjust build scripts for workspace
2. **Update CLAUDE.md** - Document new structure
3. **Create issue #54** - Link to issues #50-53 as blockers resolved
4. **Begin issues #50-53** - Terminal/Agent/Sessions/Config refactoring can now proceed within shards-core
5. **Begin CLI PRD Phase 1** - New commands go in crates/shards/
6. **Begin UI PRD Phase 1** - Add GPUI to crates/shards-ui/

---

## Notes

### Why Virtual Workspace

A virtual workspace (no root package) treats all crates as equals. This is appropriate because:
- shards-core is not "more important" than shards or shards-ui
- Each can be published/distributed independently
- No implicit hierarchy

### Why `crates/` Subdirectory

Keeps workspace root clean. Common patterns:
- `crates/` - Rust community convention
- `packages/` - npm-style
- Root-level - simpler but cluttered

We chose `crates/` as it's the Rust convention and scales well.

### Dependency Inheritance

Using `workspace.dependencies` ensures:
- Single source of truth for versions
- No version drift between crates
- Easier updates

Each crate still explicitly declares what it needs with `workspace = true`.

### Edition 2024 + Resolver 3

We're using the latest Rust features:
- `edition = "2024"` - Latest language edition
- `resolver = "3"` - Latest dependency resolver with better feature unification
