# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core Principles

**Target User:** Power users and agentic-forward engineers who want speed, control, and isolation. Users who run multiple AI agents simultaneously and need clean environment separation.

**Single-Developer Tool:** No multi-tenant complexity. Optimize for the solo developer managing parallel AI workflows.

**KISS:** Keep it simple and easily understandable over complex and "clever". First principles thinking.

**YAGNI:** Justify every line of code against the problem it solves. Is it needed?

**Type Safety (CRITICAL):** Rust's type system is a feature, not an obstacle. Use it fully.

**No Silent Failures:** This is a developer tool. Developers need to know when something fails. Never swallow errors, never hide failures behind fallbacks without logging, never leave things "behind the curtain". If config is wrong, say so. If an operation fails, surface it. Explicit failure is better than silent misbehavior.

## Git as First-Class Citizen

KILD is built around git worktrees. Let git handle what git does best:

- **Surface git errors to users** for actionable issues (conflicts, uncommitted changes, branch already exists)
- **Handle expected failures gracefully** (missing directories during cleanup, worktree already removed)
- **Trust git's natural guardrails** (e.g., git2 refuses to remove worktree with uncommitted changes - surface this, don't bypass it)
- **Branch naming:** KILD creates `kild_<branch>` branches automatically for isolation. Slashes in branch names (e.g., `feature/foo`) are supported and sanitized to hyphens for filesystem paths

## Code Quality Standards

All PRs must pass before merge:

```bash
cargo fmt --check              # Formatting (0 violations)
cargo clippy --all -- -D warnings  # Linting (0 warnings, enforced via -D)
cargo test --all               # All tests pass
cargo build --all              # Clean build
```

**Tooling:**
- `cargo fmt` - Rustfmt with default settings
- `cargo clippy` - Strict linting, warnings treated as errors
- `thiserror` - For error type definitions
- `tracing` - For structured logging (JSON output)

## Build & Development Commands

```bash
# Build
cargo build --all              # Build all crates
cargo build -p kild-core       # Build specific crate

# Test
cargo test --all               # Run all tests
cargo test -p kild-core        # Test specific crate
cargo test test_name           # Run single test by name

# Lint & Format
cargo fmt                      # Format code
cargo fmt --check              # Check formatting
cargo clippy --all -- -D warnings  # Lint with warnings as errors

# Run
cargo run -p kild -- create my-branch --agent claude
cargo run -p kild -- create my-branch --agent claude --note "Working on auth feature"
cargo run -p kild -- list
cargo run -p kild -- list --json                 # JSON output for scripting
cargo run -p kild -- status my-branch --json     # JSON output for single kild
cargo run -p kild -- -v list                     # Verbose mode (enable JSON logs)
cargo run -p kild -- cd my-branch                # Print worktree path for shell integration
cargo run -p kild -- open my-branch              # Open new agent in existing kild (additive)
cargo run -p kild -- open my-branch --agent kiro # Open with different agent
cargo run -p kild -- open --all                  # Open agents in all stopped kilds
cargo run -p kild -- open --all --agent claude   # Open all stopped kilds with specific agent
cargo run -p kild -- code my-branch              # Open worktree in editor
cargo run -p kild -- focus my-branch             # Bring terminal window to foreground
cargo run -p kild -- diff my-branch              # Show git diff for worktree
cargo run -p kild -- diff my-branch --staged     # Show only staged changes
cargo run -p kild -- commits my-branch           # Show recent commits in kild's branch
cargo run -p kild -- commits my-branch -n 5      # Show last 5 commits
cargo run -p kild -- stop my-branch              # Stop agent, preserve kild
cargo run -p kild -- stop --all                  # Stop all running kilds
cargo run -p kild -- destroy my-branch           # Destroy kild
cargo run -p kild -- destroy my-branch --force   # Force destroy (bypass git checks)
cargo run -p kild -- destroy --all               # Destroy all kilds (with confirmation)
cargo run -p kild -- destroy --all --force       # Force destroy all (skip confirmation)
cargo run -p kild -- complete my-branch          # Complete kild (check PR, cleanup)
cargo run -p kild -- complete my-branch --force  # Force complete (bypass git checks)

# kild-peek - Native app inspection
cargo run -p kild-peek -- list windows           # List all visible windows
cargo run -p kild-peek -- list windows --app Ghostty  # List windows for specific app
cargo run -p kild-peek -- list monitors          # List connected monitors
cargo run -p kild-peek -- screenshot --window "Terminal" -o /tmp/term.png
cargo run -p kild-peek -- screenshot --app Ghostty -o /tmp/ghostty.png
cargo run -p kild-peek -- screenshot --app Ghostty --window "Terminal" -o /tmp/precise.png
cargo run -p kild-peek -- screenshot --window-id 8002 -o /tmp/window.png
cargo run -p kild-peek -- diff img1.png img2.png --threshold 95
cargo run -p kild-peek -- assert --app "KILD" --exists
cargo run -p kild-peek -- assert --window "KILD" --visible
cargo run -p kild-peek -- -v list windows        # Verbose mode (enable logs)
```

## Architecture

**Workspace structure:**
- `crates/kild-core` - Core library with all business logic, no CLI dependencies
- `crates/kild` - Thin CLI that consumes kild-core (clap for arg parsing)
- `crates/kild-ui` - GPUI-based native GUI with multi-project support
- `crates/kild-peek-core` - Core library for native app inspection (window listing, screenshots, image comparison, assertions)
- `crates/kild-peek` - CLI for visual verification of native macOS applications

**Key modules in kild-core:**
- `sessions/` - Session lifecycle (create, open, stop, destroy, complete, list)
- `terminal/` - Multi-backend terminal abstraction (Ghostty, iTerm, Terminal.app)
- `agents/` - Agent backend system (amp, claude, kiro, gemini, codex)
- `git/` - Git worktree operations via git2
- `config/` - Hierarchical TOML config (defaults → user → project → CLI)
- `cleanup/` - Orphaned resource cleanup with multiple strategies
- `health/` - Session health monitoring
- `process/` - PID tracking and process info
- `logging/` - Tracing initialization with JSON output
- `events/` - App lifecycle event helpers

**Key modules in kild-ui:**
- `theme.rs` - Centralized color palette, typography, and spacing constants (Tallinn Night brand system)
- `components/` - Reusable UI components (Button, StatusIndicator, Modal, TextInput with themed variants)
- `projects.rs` - Project storage, validation, persistence to ~/.kild/projects.json
- `state.rs` - Type-safe state modules with encapsulated AppState facade (DialogState, ProjectManager, SessionStore, SelectionState, OperationErrors)
- `actions.rs` - User actions (create, open, stop, destroy, project management)
- `views/` - GPUI components (main view with 3-column layout: sidebar, kild list, detail panel)
- `watcher.rs` - File system watcher for instant UI updates on session changes
- `refresh.rs` - Background refresh logic with hybrid file watching + slow poll fallback

**Key modules in kild-peek-core:**
- `window/` - Window and monitor enumeration via macOS APIs
- `screenshot/` - Screenshot capture with multiple targets (window, monitor, base64 output)
- `diff/` - Image comparison using SSIM algorithm
- `assert/` - UI state assertions (window exists, visible, image similarity)
- `logging/` - Tracing initialization matching kild-core patterns
- `events/` - App lifecycle event helpers

**Module pattern:** Each domain follows `errors.rs`, `types.rs`, `operations.rs`, `handler.rs` structure.

**CLI interaction:** Commands delegate directly to `kild-core` handlers. No business logic in CLI layer.

## Code Style Preferences

**Prefer string literals over enums for event names.** Event names are typed as string literals directly in the tracing macros, not as enum variants. This keeps logging flexible and greppable.

## Structured Logging

### Setup

Logging is initialized via `kild_core::init_logging(quiet)` in the CLI main.rs. Output is JSON format via tracing-subscriber.

By default, only error-level events are emitted (clean output). When `-v/--verbose` flag is used, info-level and above events are emitted.

Control log level with `RUST_LOG` env var: `RUST_LOG=debug cargo run -- list`

Enable verbose logs with the verbose flag: `cargo run -- -v list`

### Event Naming Convention

All events follow: `{layer}.{domain}.{action}_{state}`

| Layer | Crate | Description |
|-------|-------|-------------|
| `cli` | `crates/kild/` | User-facing CLI commands |
| `core` | `crates/kild-core/` | Core library logic |
| `ui` | `crates/kild-ui/` | GPUI native GUI |
| `peek.cli` | `crates/kild-peek/` | kild-peek CLI commands |
| `peek.core` | `crates/kild-peek-core/` | kild-peek core library |

**Domains:** `session`, `terminal`, `git`, `cleanup`, `health`, `files`, `process`, `pid_file`, `app`, `projects`, `watcher`, `window`, `screenshot`, `diff`, `assert`

**State suffixes:** `_started`, `_completed`, `_failed`, `_skipped`

### Logging Examples

```rust
// CLI layer - simple events
info!(event = "cli.create_started", branch = branch, agent = config.agent.default);
info!(event = "cli.create_completed", session_id = session.id, branch = session.branch);
error!(event = "cli.create_failed", error = %e);

// Core layer - domain-prefixed events
info!(event = "core.session.create_started", branch = request.branch, agent = agent);
info!(event = "core.session.create_completed", session_id = session.id);
warn!(event = "core.session.agent_not_available", agent = agent);

// Sub-domains for nested concepts
info!(event = "core.git.worktree.create_started", branch = branch);
info!(event = "core.git.worktree.create_completed", path = %worktree_path.display());
info!(event = "core.git.branch.create_completed", branch = branch);

// Debug level for internal operations
debug!(event = "core.pid_file.read_attempt", attempt = attempt, path = %pid_file.display());
debug!(event = "core.terminal.applescript_executing", terminal = terminal_name);

// UI layer - watcher domain for file system events
info!(event = "ui.watcher.started", path = %sessions_dir.display());
warn!(event = "ui.watcher.create_failed", error = %e, "File watcher unavailable");
debug!(event = "ui.watcher.event_detected", kind = ?event.kind, paths = ?event.paths);

// Structured fields - use Display (%e) for errors, Debug (?val) for complex types
error!(event = "core.session.destroy_kill_failed", pid = pid, error = %e);
warn!(event = "core.files.walk.error", error = %e, path = %path.display());
```

### App Lifecycle Events

Use helpers from `kild_core::events`:

```rust
use kild_core::events;

events::log_app_startup();           // core.app.startup_completed
events::log_app_shutdown();          // core.app.shutdown_started
events::log_app_error(&error);       // core.app.error_occurred
```

### Log Level Guidelines

| Level | Usage |
|-------|-------|
| `error!` | Operation failed, requires attention |
| `warn!` | Degraded operation, fallback used, non-critical issues |
| `info!` | Operation lifecycle (_started, _completed), user-relevant events |
| `debug!` | Internal state, retry attempts, detailed flow |

### Filtering Logs

```bash
# By layer
grep 'event":"core\.'      # Core library events
grep 'event":"cli\.'       # CLI events
grep 'event":"ui\.'        # GUI events
grep 'event":"peek\.core\.' # kild-peek core events
grep 'event":"peek\.cli\.'  # kild-peek CLI events

# By domain
grep 'core\.session\.'  # Session events
grep 'core\.terminal\.' # Terminal events
grep 'core\.git\.'      # Git events
grep 'ui\.projects\.'   # Project management events
grep 'ui\.watcher\.'    # File watcher events
grep 'peek\.core\.window\.'     # Window enumeration events
grep 'peek\.core\.screenshot\.' # Screenshot capture events

# By outcome
grep '_failed"'         # All failures
grep '_completed"'      # All completions
grep '_started"'        # All operation starts
```

## Terminal Backend Pattern

```rust
pub trait TerminalBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn execute_spawn(&self, config: &SpawnConfig, window_title: Option<&str>)
        -> Result<Option<String>, TerminalError>;
    fn focus_window(&self, window_id: Option<&str>) -> Result<(), TerminalError>;
    fn close_window(&self, window_id: Option<&str>);
    fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError>;
}
```

Backends registered in `terminal/registry.rs`. Detection preference: Ghostty > iTerm > Terminal.app.

Status detection uses PID tracking by default. Ghostty uses window-based detection as fallback when PID is unavailable.

## Configuration Hierarchy

Priority (highest wins): CLI args → project config (`./.kild/config.toml`) → user config (`~/.kild/config.toml`) → defaults

**Array Merging:** `include_patterns.patterns` arrays are merged (deduplicated) from user and project configs. Other config values follow standard override behavior.

## Error Handling

All domain errors implement `KildError` trait with `error_code()` and `is_user_error()`. Use `thiserror` for definitions.
