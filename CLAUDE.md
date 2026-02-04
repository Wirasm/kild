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
- **Branch naming:** KILD creates `kild/<branch>` branches automatically for isolation using git-native namespacing. The worktree admin name (`kild-<branch>`) is filesystem-safe and decoupled from the branch name via `WorktreeAddOptions::reference()`

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

# kild-peek - Native app inspection and interaction
cargo run -p kild-peek -- list windows           # List all visible windows
cargo run -p kild-peek -- list windows --app Ghostty  # List windows for specific app
cargo run -p kild-peek -- list monitors          # List connected monitors
cargo run -p kild-peek -- screenshot --window "Terminal" -o /tmp/term.png
cargo run -p kild-peek -- screenshot --app Ghostty -o /tmp/ghostty.png
cargo run -p kild-peek -- screenshot --app Ghostty --window "Terminal" -o /tmp/precise.png
cargo run -p kild-peek -- screenshot --window-id 8002 -o /tmp/window.png
cargo run -p kild-peek -- screenshot --window "Terminal" --wait -o /tmp/term.png  # Wait for window
cargo run -p kild-peek -- screenshot --window "Terminal" --wait --timeout 5000 -o /tmp/term.png  # Custom timeout
cargo run -p kild-peek -- screenshot --app Ghostty --crop 0,0,400,300 -o /tmp/cropped.png
cargo run -p kild-peek -- diff img1.png img2.png --threshold 95
cargo run -p kild-peek -- diff img1.png img2.png --diff-output /tmp/diff.png
cargo run -p kild-peek -- elements --app Finder                  # List all UI elements
cargo run -p kild-peek -- elements --window "Terminal" --json    # JSON output
cargo run -p kild-peek -- elements --app Finder --tree           # Display as indented tree hierarchy
cargo run -p kild-peek -- elements --app Finder --wait           # Wait for window to appear
cargo run -p kild-peek -- elements --app Finder --wait --timeout 5000  # Custom timeout
cargo run -p kild-peek -- find --app Finder --text "File"        # Find element by text
cargo run -p kild-peek -- find --app KILD --text "Create" --json # JSON output
cargo run -p kild-peek -- find --app Finder --text "^File$" --regex  # Find by regex pattern
cargo run -p kild-peek -- find --app KILD --text "Create" --wait # Wait for window to appear
cargo run -p kild-peek -- click --window "Terminal" --at 100,50  # Click at coordinates (x,y)
cargo run -p kild-peek -- click --app Ghostty --at 200,100      # Target by app name
cargo run -p kild-peek -- click --app Ghostty --window "Terminal" --at 150,75  # Target both
cargo run -p kild-peek -- click --window "Terminal" --at 100,50 --json  # JSON output
cargo run -p kild-peek -- click --app KILD --text "Create"       # Click element by text
cargo run -p kild-peek -- click --app Finder --text "File" --json  # Click by text, JSON output
cargo run -p kild-peek -- click --app KILD --text "Create" --wait  # Wait for window to appear
cargo run -p kild-peek -- click --app Finder --at 100,50 --right  # Right-click (context menu)
cargo run -p kild-peek -- click --app Finder --at 100,50 --double # Double-click
cargo run -p kild-peek -- click --app Finder --text "File" --right  # Right-click element by text
cargo run -p kild-peek -- drag --app Finder --from 100,100 --to 300,200  # Drag from point to point
cargo run -p kild-peek -- drag --app Finder --from 10,20 --to 30,40 --json  # JSON output
cargo run -p kild-peek -- scroll --app Finder --down 5            # Scroll down 5 lines
cargo run -p kild-peek -- scroll --app Finder --up 3              # Scroll up 3 lines
cargo run -p kild-peek -- scroll --app Finder --left 2            # Scroll left 2 lines
cargo run -p kild-peek -- scroll --app Finder --right 4           # Scroll right 4 lines
cargo run -p kild-peek -- scroll --app Finder --at 100,200 --down 5  # Scroll at position
cargo run -p kild-peek -- hover --app Finder --at 100,50          # Move mouse without clicking
cargo run -p kild-peek -- hover --app Finder --text "File"        # Hover over element by text
cargo run -p kild-peek -- hover --app Finder --at 50,50 --json    # JSON output
cargo run -p kild-peek -- type --window "Terminal" "hello world"  # Type text
cargo run -p kild-peek -- type --app TextEdit "some text"         # Target by app
cargo run -p kild-peek -- type --window "Terminal" "test" --json  # JSON output
cargo run -p kild-peek -- type --app TextEdit "text" --wait       # Wait for window to appear
cargo run -p kild-peek -- key --window "Terminal" "enter"         # Single key
cargo run -p kild-peek -- key --app Ghostty "cmd+s"               # Key combo
cargo run -p kild-peek -- key --window "Terminal" "cmd+shift+p"   # Multiple modifiers
cargo run -p kild-peek -- key --app TextEdit "tab" --json         # JSON output
cargo run -p kild-peek -- key --app Ghostty "enter" --wait        # Wait for window to appear
cargo run -p kild-peek -- assert --app "KILD" --exists
cargo run -p kild-peek -- assert --window "KILD" --visible
cargo run -p kild-peek -- assert --window "KILD" --exists --wait  # Wait for window to appear
cargo run -p kild-peek -- assert --window "KILD" --exists --wait --timeout 5000  # Custom timeout
cargo run -p kild-peek -- -v list windows        # Verbose mode (enable logs)
```

## Architecture

**Workspace structure:**
- `crates/kild-core` - Core library with all business logic, no CLI dependencies
- `crates/kild` - Thin CLI that consumes kild-core (clap for arg parsing)
- `crates/kild-ui` - GPUI-based native GUI with multi-project support
- `crates/kild-peek-core` - Core library for native app inspection and interaction (window listing, screenshots, image comparison, assertions, UI automation)
- `crates/kild-peek` - CLI for visual verification of native macOS applications

**Key modules in kild-core:**
- `sessions/` - Session lifecycle (create, open, stop, destroy, complete, list)
- `terminal/` - Multi-backend terminal abstraction (Ghostty, iTerm, Terminal.app, Alacritty)
- `agents/` - Agent backend system (amp, claude, kiro, gemini, codex, opencode)
- `git/` - Git worktree operations via git2
- `config/` - Hierarchical TOML config (defaults → user → project → CLI)
- `projects/` - Project management (types, validation, persistence, manager)
- `cleanup/` - Orphaned resource cleanup with multiple strategies
- `health/` - Session health monitoring
- `process/` - PID tracking and process info
- `logging/` - Tracing initialization with JSON output
- `events/` - App lifecycle event helpers
- `state/` - Command pattern for business operations (Command enum, Event enum, Store trait returns events)

**Key modules in kild-ui:**
- `theme.rs` - Centralized color palette, typography, and spacing constants (Tallinn Night brand system)
- `components/` - Reusable UI components (Button, StatusIndicator, Modal, TextInput with themed variants)
- `state/` - Type-safe state modules with encapsulated AppState facade (app_state.rs, dialog.rs, errors.rs, loading.rs, selection.rs, sessions.rs)
- `actions.rs` - User actions (create, open, stop, destroy, project management)
- `views/` - GPUI components (main view with 3-column layout: sidebar, kild list, detail panel)
- `watcher.rs` - File system watcher for instant UI updates on session changes
- `refresh.rs` - Background refresh logic with hybrid file watching + slow poll fallback

**Key modules in kild-peek-core:**
- `window/` - Window and monitor enumeration via macOS APIs
- `screenshot/` - Screenshot capture with multiple targets (window, monitor, base64 output)
- `diff/` - Image comparison using SSIM algorithm
- `assert/` - UI state assertions (window exists, visible, image similarity)
- `interact/` - Native UI interaction (mouse clicks, keyboard input, key combinations, text-based clicking)
- `element/` - Accessibility API-based element enumeration, text search, and element finding
- `logging/` - Tracing initialization matching kild-core patterns
- `events/` - App lifecycle event helpers

**Module pattern:** Each domain starts with `errors.rs`, `types.rs`, `mod.rs`. Additional files vary by domain (e.g., `handler.rs`/`persistence.rs`/`validation.rs` for sessions, `manager.rs`/`persistence.rs` for projects).

**CLI interaction:** Commands delegate directly to `kild-core` handlers. No business logic in CLI layer.

**Command pattern:** Business operations are defined as `Command` enum variants in `kild-core/state/types.rs`. The `Store` trait in `kild-core/state/store.rs` provides the dispatch contract, returning `Vec<Event>` on success to describe state changes. The `Event` enum in `kild-core/state/events.rs` defines all business state changes (kild lifecycle, project management).

**Dispatch vs direct-call guidance:**
- **UI state-mutating operations**: Always dispatch through Store. All operations use `Command` → `CoreStore::dispatch` → `Event` → `apply_events`. This ensures consistent event-driven state updates.
- **CLI operations**: Call handler functions directly (e.g., `session_ops::create_session`). The CLI is synchronous and doesn't need event-driven updates.
- **Read-only queries**: Call handler functions directly from both CLI and UI. Queries like `list_sessions`, `get_session`, `get_destroy_safety_info` don't mutate state and don't need dispatch.

## Code Style Preferences

**Prefer string literals over enums for event names.** Event names are typed as string literals directly in the tracing macros, not as enum variants. This keeps logging flexible and greppable.

**No backward-compatibility shims.** When renaming or moving types, rename all usages. Never introduce type aliases, re-exports, or wrapper types solely for backward compatibility. This is a single-developer tool with no external consumers — there is nobody to keep compatible with. One name, one type, everywhere.

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

**Domains:** `session`, `terminal`, `git`, `cleanup`, `health`, `files`, `process`, `pid_file`, `app`, `projects`, `state`, `watcher`, `window`, `screenshot`, `diff`, `assert`, `interact`, `element`

Note: `projects` domain events are `core.projects.*` (in kild-core), while UI-specific events use `ui.*` prefix.

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
grep 'core\.projects\.' # Project management events
grep 'ui\.watcher\.'    # File watcher events
grep 'peek\.core\.window\.'     # Window enumeration events
grep 'peek\.core\.screenshot\.' # Screenshot capture events
grep 'peek\.core\.interact\.'   # UI interaction events
grep 'peek\.core\.element\.'    # Element enumeration events

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

Backends registered in `terminal/registry.rs`. Detection preference varies by platform:
- macOS: Ghostty > iTerm > Terminal.app
- Linux: Alacritty (requires Hyprland window manager)

Status detection uses PID tracking by default. Ghostty uses window-based detection as fallback when PID is unavailable. Alacritty on Linux uses Hyprland IPC for window management.

## Configuration Hierarchy

Priority (highest wins): CLI args → project config (`./.kild/config.toml`) → user config (`~/.kild/config.toml`) → defaults

**Array Merging:** `include_patterns.patterns` arrays are merged (deduplicated) from user and project configs. Other config values follow standard override behavior.

## Error Handling

All domain errors implement `KildError` trait with `error_code()` and `is_user_error()`. Use `thiserror` for definitions.
