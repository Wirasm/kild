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

Shards is built around git worktrees. Let git handle what git does best:

- **Surface git errors to users** for actionable issues (conflicts, uncommitted changes, branch already exists)
- **Handle expected failures gracefully** (missing directories during cleanup, worktree already removed)
- **Trust git's natural guardrails** (e.g., git2 refuses to remove worktree with uncommitted changes - surface this, don't bypass it)
- **Branch naming:** Shards creates `shard_<hash>` branches automatically for isolation

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
cargo build -p shards-core     # Build specific crate

# Test
cargo test --all               # Run all tests
cargo test -p shards-core      # Test specific crate
cargo test test_name           # Run single test by name

# Lint & Format
cargo fmt                      # Format code
cargo fmt --check              # Check formatting
cargo clippy --all -- -D warnings  # Lint with warnings as errors

# Run
cargo run -- create my-branch --agent claude
cargo run -- create my-branch --agent claude --note "Working on auth feature"
cargo run -- list
cargo run -- list --json                 # JSON output for scripting
cargo run -- status my-branch --json     # JSON output for single shard
cargo run -- -q list                     # Quiet mode (suppress JSON logs)
cargo run -- cd my-branch                # Print worktree path for shell integration
cargo run -- open my-branch              # Open new agent in existing shard (additive)
cargo run -- open my-branch --agent kiro # Open with different agent
cargo run -- open --all                  # Open agents in all stopped shards
cargo run -- open --all --agent claude   # Open all stopped shards with specific agent
cargo run -- code my-branch              # Open worktree in editor
cargo run -- focus my-branch             # Bring terminal window to foreground
cargo run -- stop my-branch              # Stop agent, preserve shard
cargo run -- stop --all                  # Stop all running shards
cargo run -- destroy my-branch           # Destroy shard
cargo run -- destroy my-branch --force   # Force destroy (bypass git checks)
```

## Architecture

**Workspace structure:**
- `crates/shards-core` - Core library with all business logic, no CLI dependencies
- `crates/shards` - Thin CLI that consumes shards-core (clap for arg parsing)
- `crates/shards-ui` - GPUI-based native GUI (in development)

**Key modules in shards-core:**
- `sessions/` - Session lifecycle (create, open, stop, destroy, list)
- `terminal/` - Multi-backend terminal abstraction (Ghostty, iTerm, Terminal.app)
- `agents/` - Agent backend system (claude, kiro, gemini, etc.)
- `git/` - Git worktree operations via git2
- `config/` - Hierarchical TOML config (defaults → user → project → CLI)
- `cleanup/` - Orphaned resource cleanup with multiple strategies
- `health/` - Session health monitoring
- `process/` - PID tracking and process info
- `logging/` - Tracing initialization with JSON output
- `events/` - App lifecycle event helpers

**Module pattern:** Each domain follows `errors.rs`, `types.rs`, `operations.rs`, `handler.rs` structure.

**CLI interaction:** Commands delegate directly to `shards-core` handlers. No business logic in CLI layer.

## Code Style Preferences

**Prefer string literals over enums for event names.** Event names are typed as string literals directly in the tracing macros, not as enum variants. This keeps logging flexible and greppable.

## Structured Logging

### Setup

Logging is initialized via `shards_core::init_logging(quiet)` in the CLI main.rs. Output is JSON format via tracing-subscriber.

When `quiet` is true (via `-q` flag), only error-level events are emitted. When false, info-level and above events are emitted.

Control log level with `RUST_LOG` env var: `RUST_LOG=debug cargo run -- list`

Suppress logs entirely with the quiet flag: `cargo run -- -q list`

### Event Naming Convention

All events follow: `{layer}.{domain}.{action}_{state}`

| Layer | Crate | Description |
|-------|-------|-------------|
| `cli` | `crates/shards/` | User-facing CLI commands |
| `core` | `crates/shards-core/` | Core library logic |

**Domains:** `session`, `terminal`, `git`, `cleanup`, `health`, `files`, `process`, `pid_file`, `app`

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

// Structured fields - use Display (%e) for errors, Debug (?val) for complex types
error!(event = "core.session.destroy_kill_failed", pid = pid, error = %e);
warn!(event = "core.files.walk.error", error = %e, path = %path.display());
```

### App Lifecycle Events

Use helpers from `shards_core::events`:

```rust
use shards_core::events;

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
grep 'event":"core\.'   # Core library events
grep 'event":"cli\.'    # CLI events

# By domain
grep 'core\.session\.'  # Session events
grep 'core\.terminal\.' # Terminal events
grep 'core\.git\.'      # Git events

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
}
```

Backends registered in `terminal/registry.rs`. Detection preference: Ghostty > iTerm > Terminal.app.

## Configuration Hierarchy

Priority (highest wins): CLI args → project config (`./shards/config.toml`) → user config (`~/.shards/config.toml`) → defaults

## Error Handling

All domain errors implement `ShardsError` trait with `error_code()` and `is_user_error()`. Use `thiserror` for definitions.
