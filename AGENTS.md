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
cargo build -p kild-protocol   # Build protocol types crate

# Test
cargo test --all               # Run all tests
cargo test -p kild-core        # Test specific crate
cargo test -p kild-protocol    # Test protocol types crate
cargo test test_name           # Run single test by name

# Lint & Format
cargo fmt                      # Format code
cargo fmt --check              # Check formatting
cargo clippy --all -- -D warnings  # Lint with warnings as errors

# Run
cargo run -p kild -- create my-branch --agent claude
cargo run -p kild -- create my-branch --agent claude --note "Working on auth feature"
cargo run -p kild -- create my-branch --no-agent       # Open bare terminal with $SHELL
cargo run -p kild -- create my-branch --daemon         # Launch in daemon-owned PTY
cargo run -p kild -- create my-branch --no-daemon      # Force external terminal (override config)
cargo run -p kild -- list
cargo run -p kild -- list --json                 # JSON object with sessions array and fleet_summary
cargo run -p kild -- status my-branch --json     # JSON output for single kild
cargo run -p kild -- -v list                     # Verbose mode (enable JSON logs)
cargo run -p kild -- cd my-branch                # Print worktree path for shell integration
cargo run -p kild -- open my-branch              # Open new agent in existing kild (auto-detects runtime mode from session)
cargo run -p kild -- open my-branch --agent kiro # Open with different agent
cargo run -p kild -- open my-branch --no-agent   # Open bare terminal with $SHELL (no agent)
cargo run -p kild -- open my-branch --resume     # Resume previous agent session (restore conversation context)
cargo run -p kild -- open my-branch -r           # Short form of --resume
cargo run -p kild -- open my-branch --daemon     # Override: force daemon-owned PTY
cargo run -p kild -- open my-branch --no-daemon  # Override: force external terminal
cargo run -p kild -- open --all                  # Open agents in all stopped kilds
cargo run -p kild -- open --all --agent claude   # Open all stopped kilds with specific agent
cargo run -p kild -- open --all --no-agent       # Open bare terminals in all stopped kilds
cargo run -p kild -- open --all --resume         # Resume all stopped kilds with previous session context
cargo run -p kild -- code my-branch              # Open worktree in editor (config > $EDITOR > code)
cargo run -p kild -- code my-branch --editor vim # Override editor
cargo run -p kild -- focus my-branch             # Bring terminal window to foreground
cargo run -p kild -- hide my-branch              # Minimize/hide terminal window
cargo run -p kild -- hide --all                  # Hide all active kild windows
cargo run -p kild -- diff my-branch              # Show git diff for worktree
cargo run -p kild -- diff my-branch --staged     # Show only staged changes
cargo run -p kild -- diff my-branch --stat       # Show diffstat summary
cargo run -p kild -- commits my-branch           # Show recent commits in kild's branch
cargo run -p kild -- commits my-branch -n 5      # Show last 5 commits
cargo run -p kild -- stats my-branch             # Show branch health and merge readiness
cargo run -p kild -- stats my-branch --json      # JSON output for branch health
cargo run -p kild -- stats my-branch -b dev      # Override base branch
cargo run -p kild -- stats --all                 # Fleet summary table (all kilds)
cargo run -p kild -- stats --all --json          # JSON array of all kild health
cargo run -p kild -- overlaps                    # Detect file overlaps across kilds
cargo run -p kild -- overlaps --json             # JSON output for overlap detection
cargo run -p kild -- overlaps -b dev             # Override base branch
cargo run -p kild -- pr my-branch                # Show PR status for kild
cargo run -p kild -- pr my-branch --json         # JSON output for PR status
cargo run -p kild -- pr my-branch --refresh      # Force refresh PR data from GitHub
cargo run -p kild -- rebase my-branch            # Rebase kild branch onto base branch
cargo run -p kild -- rebase my-branch --base dev # Rebase onto custom base branch
cargo run -p kild -- rebase --all                # Rebase all kilds onto base branch
cargo run -p kild -- sync my-branch              # Fetch + rebase kild branch
cargo run -p kild -- sync my-branch --base dev   # Fetch + rebase onto custom base
cargo run -p kild -- sync --all                  # Fetch once + rebase all kilds
cargo run -p kild -- agent-status my-branch working  # Report agent activity (for hooks)
cargo run -p kild -- agent-status --self idle        # Auto-detect session from $PWD
cargo run -p kild -- agent-status my-branch waiting --notify  # Send desktop notification for attention states
cargo run -p kild -- daemon start                # Start daemon in background
cargo run -p kild -- daemon start --foreground   # Start daemon in foreground (debug)
cargo run -p kild -- daemon stop                 # Stop running daemon
cargo run -p kild -- daemon status               # Show daemon status
cargo run -p kild -- daemon status --json        # JSON output for daemon status
cargo run -p kild -- attach my-branch            # Attach to daemon-managed kild (Ctrl+C to detach)
cargo run -p kild -- stop my-branch              # Stop agent, preserve kild
cargo run -p kild -- stop --all                  # Stop all running kilds
cargo run -p kild -- destroy my-branch           # Destroy kild
cargo run -p kild -- destroy my-branch --force   # Force destroy (bypass git checks)
cargo run -p kild -- destroy --all               # Destroy all kilds (with confirmation)
cargo run -p kild -- destroy --all --force       # Force destroy all (skip confirmation)
cargo run -p kild -- complete my-branch          # Complete kild (check PR, cleanup)
cargo run -p kild -- completions bash            # Generate bash completions
cargo run -p kild -- completions zsh             # Generate zsh completions
cargo run -p kild -- completions fish            # Generate fish completions
cargo run -p kild -- completions powershell      # Generate powershell completions
cargo run -p kild -- completions elvish          # Generate elvish completions

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

# kild-tmux-shim - tmux compatibility shim for agent teams
cargo run -p kild-tmux-shim -- -V                # Show version (reports "tmux 3.4")
KILD_SHIM_SESSION=<session_id> TMUX_PANE=%0 cargo run -p kild-tmux-shim -- display-message -p "#{pane_id}"  # Test shim
KILD_SHIM_LOG=1 cargo run -p kild-tmux-shim -- <command>  # Enable file-based logging
```

**Note**: The shim is automatically symlinked as `~/.kild/bin/tmux` when creating daemon sessions. Manual invocation is primarily for testing.

## Architecture

**Workspace structure:**

- `crates/kild-protocol` - Shared IPC protocol types (ClientMessage, DaemonMessage, SessionInfo, SessionStatus, ErrorCode). Deps: serde, serde_json only. No tokio, no kild-core. Single source of truth for daemon wire format.
- `crates/kild-core` - Core library with all business logic, no CLI dependencies
- `crates/kild` - Thin CLI that consumes kild-core (clap for arg parsing)
- `crates/kild-daemon` - Standalone daemon binary for PTY management (async tokio server, JSONL IPC protocol, portable-pty integration). CLI spawns this as subprocess. Wire types re-exported from kild-protocol.
- `crates/kild-tmux-shim` - tmux-compatible shim binary for agent team support (CLI that intercepts tmux commands, routes to daemon IPC)
- `crates/kild-ui` - GPUI-based native GUI with multi-project support
- `crates/kild-peek-core` - Core library for native app inspection and interaction (window listing, screenshots, image comparison, assertions, UI automation)
- `crates/kild-peek` - CLI for visual verification of native macOS applications

**Key modules in kild-core:**

- `sessions/` - Session lifecycle (create, open, stop, destroy, complete, list)
- `terminal/` - Multi-backend terminal abstraction (Ghostty, iTerm, Terminal.app, Alacritty)
- `agents/` - Agent backend system (amp, claude, kiro, gemini, codex, opencode, resume.rs for session continuity)
- `daemon/` - Daemon client for IPC communication (sync Unix socket client) and auto-start logic (discovers kild-daemon binary as sibling executable)
- `git/` - Git worktree operations via git2
- `forge/` - Forge backend system (GitHub, future: GitLab, Bitbucket, Gitea) for PR operations
- `config/` - Hierarchical TOML config (defaults → user → project → CLI)
- `projects/` - Project management (types, validation, persistence, manager)
- `cleanup/` - Orphaned resource cleanup with multiple strategies
- `health/` - Session health monitoring
- `process/` - PID tracking and process info
- `logging/` - Tracing initialization with JSON output
- `events/` - App lifecycle event helpers
- `notify/` - Platform-native desktop notifications (macOS, Linux)
- `state/` - Command pattern for business operations (Command enum, Event enum, Store trait returns events, RuntimeMode enum)

**Key modules in kild-ui:**

- `theme.rs` - Centralized color palette, typography, and spacing constants (Tallinn Night brand system)
- `theme_bridge.rs` - Maps Tallinn Night colors to gpui-component theme tokens
- `components/` - Custom UI components (StatusIndicator only; Button, TextInput, Modal from gpui-component library)
- `state/` - Type-safe state modules with encapsulated AppState facade (app_state.rs, dialog.rs, errors.rs, loading.rs, selection.rs, sessions.rs)
- `actions.rs` - User actions (create, open, stop, destroy, project management)
- `views/` - GPUI components (permanent Rail | Sidebar | Main layout with project_rail.rs for 48px project switcher, sidebar.rs for kild navigation grouped by Active/Stopped, ActiveView enum for Control/Dashboard/Detail tab bar, dashboard_view.rs for fleet overview cards, detail_view.rs for kild drill-down, terminal_tabs.rs for multi-terminal support)
- `terminal/` - Live terminal rendering with PTY integration (state.rs for PTY lifecycle, terminal_element.rs for GPUI Element, terminal_view.rs for View, colors.rs for ANSI mapping, input.rs for keystroke translation)
- `watcher.rs` - File system watcher for instant UI updates on session changes
- `refresh.rs` - Background refresh logic with hybrid file watching + slow poll fallback

**Key modules in kild-daemon:**

- `protocol/` - JSONL IPC protocol (ClientMessage, DaemonMessage, codec)
- `pty/` - PTY lifecycle management (PtyManager, ManagedPty via portable-pty, output broadcasting)
- `session/` - Daemon session state machine (SessionManager, DaemonSession, SessionState enum)
- `server/` - Unix socket server (async connection handling, message dispatch, signal-based shutdown)
- `client/` - Daemon client for typed IPC operations (DaemonClient with connection pooling)

**Key modules in kild-peek-core:**

- `window/` - Window and monitor enumeration via macOS APIs
- `screenshot/` - Screenshot capture with multiple targets (window, monitor, base64 output)
- `diff/` - Image comparison using SSIM algorithm
- `assert/` - UI state assertions (window exists, visible, image similarity)
- `interact/` - Native UI interaction (mouse clicks, keyboard input, key combinations, text-based clicking)
- `element/` - Accessibility API-based element enumeration, text search, and element finding
- `logging/` - Tracing initialization matching kild-core patterns
- `events/` - App lifecycle event helpers

**Key modules in kild-tmux-shim:**

- `parser.rs` - Hand-rolled tmux argument parser for ~15 subcommands + aliases
- `commands.rs` - Command handlers dispatching to daemon IPC or local state
- `state.rs` - File-based pane registry with flock concurrency control
- `ipc.rs` - Sync JSONL client over Unix socket (no kild-core dependency)
- `main.rs` - Entry point, file-based logging controlled by KILD_SHIM_LOG env var
- `errors.rs` - ShimError type

**Module pattern:** Each domain in kild-core starts with `errors.rs`, `types.rs`, `mod.rs`. Additional files vary by domain (e.g., `create.rs`/`open.rs`/`stop.rs`/`list.rs`/`destroy.rs`/`complete.rs`/`agent_status.rs`/`daemon_helpers.rs` for sessions with `handler.rs` as re-export facade, `manager.rs`/`persistence.rs` for projects). kild-daemon uses a flatter structure with top-level errors/types and module-specific implementation files. kild-tmux-shim is a flat CLI crate with per-module files (no nested mod structure).

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

By default, all log output is suppressed (clean output). When `-v/--verbose` flag is used, info-level and above events are emitted.

The `-v/--verbose` flag is required to see any logs. The `RUST_LOG` env var alone will not override the default quiet mode.

Enable verbose logs with the verbose flag: `cargo run -- -v list`

### Event Naming Convention

All events follow: `{layer}.{domain}.{action}_{state}`

| Layer       | Crate                    | Description                      |
| ----------- | ------------------------ | -------------------------------- |
| `cli`       | `crates/kild/`           | User-facing CLI commands         |
| `core`      | `crates/kild-core/`      | Core library logic               |
| `daemon`    | `crates/kild-daemon/`    | Daemon server and PTY management |
| `shim`      | `crates/kild-tmux-shim/` | tmux shim binary operations      |
| `ui`        | `crates/kild-ui/`        | GPUI native GUI                  |
| `peek.cli`  | `crates/kild-peek/`      | kild-peek CLI commands           |
| `peek.core` | `crates/kild-peek-core/` | kild-peek core library           |

**Domains:** `session`, `terminal`, `daemon`, `git`, `forge`, `cleanup`, `health`, `files`, `process`, `pid_file`, `app`, `projects`, `state`, `notify`, `watcher`, `window`, `screenshot`, `diff`, `assert`, `interact`, `element`, `pty`, `protocol`, `split_window`, `send_keys`, `list_panes`, `kill_pane`, `display_message`, `select_pane`, `set_option`, `select_layout`, `resize_pane`, `has_session`, `new_session`, `new_window`, `list_windows`, `break_pane`, `join_pane`, `capture_pane`, `ipc`

UI-specific domains: `terminal` (for kild-ui terminal rendering), `input` (for keystroke translation)

Note: `projects` domain events are `core.projects.*` (in kild-core), while UI-specific events use `ui.*` prefix.

Note: `core.daemon.*` = daemon client IPC and auto-start (in kild-core), `daemon.*` = daemon server/PTY operations (in kild-daemon).

Daemon server sub-domains: `session`, `pty`, `server`, `connection`, `client`, `pid`, `config`

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

// Forge domain for PR operations
info!(event = "core.forge.pr_merge_check_started", branch = branch);
info!(event = "core.forge.pr_merge_check_completed", merged = true);
info!(event = "core.forge.pr_info_fetch_completed", pr_number = pr.number, state = ?pr.state);

// Notify domain for desktop notifications
info!(event = "core.notify.send_started", title = title, message = message);
info!(event = "core.notify.send_completed", title = title);
warn!(event = "core.notify.send_failed", title = title, error = %e);

// Debug level for internal operations
debug!(event = "core.pid_file.read_attempt", attempt = attempt, path = %pid_file.display());
debug!(event = "core.terminal.applescript_executing", terminal = terminal_name);

// UI layer - watcher domain for file system events
info!(event = "ui.watcher.started", path = %sessions_dir.display());
warn!(event = "ui.watcher.create_failed", error = %e, "File watcher unavailable");
debug!(event = "ui.watcher.event_detected", kind = ?event.kind, paths = ?event.paths);

// Daemon auto-start (core layer)
info!(event = "core.daemon.auto_start_started");
info!(event = "core.daemon.auto_start_completed");
error!(event = "core.daemon.auto_start_failed", reason = "child_exited", status = %status);
warn!(event = "core.daemon.ping_check_failed", error = %e);

// CLI daemon operations
info!(event = "cli.daemon.start_started", foreground = foreground);
info!(event = "cli.attach_started", branch = branch);

// Codex notify hook integration
info!(event = "core.session.codex_notify_hook_installed", path = %hook_path.display());
warn!(event = "core.session.codex_notify_hook_failed", error = %msg);
info!(event = "core.session.codex_config_patched", path = %config_path.display());
info!(event = "core.session.codex_config_already_configured");
warn!(event = "core.session.codex_config_patch_failed", error = %msg);

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

| Level    | Usage                                                              |
| -------- | ------------------------------------------------------------------ |
| `error!` | Operation failed, requires attention                               |
| `warn!`  | Degraded operation, fallback used, non-critical issues             |
| `info!`  | Operation lifecycle (\_started, \_completed), user-relevant events |
| `debug!` | Internal state, retry attempts, detailed flow                      |

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
grep 'core\.daemon\.'   # Daemon client events
grep 'core\.git\.'      # Git events
grep 'core\.forge\.'    # Forge/PR events
grep 'core\.projects\.' # Project management events
grep 'core\.notify\.'   # Desktop notification events
grep 'ui\.watcher\.'    # File watcher events
grep 'peek\.core\.window\.'     # Window enumeration events
grep 'peek\.core\.screenshot\.' # Screenshot capture events
grep 'peek\.core\.interact\.'   # UI interaction events
grep 'peek\.core\.element\.'    # Element enumeration events

# Daemon server events
grep 'event":"daemon\.'   # Daemon server events
grep 'daemon\.session\.'     # Session state machine events
grep 'daemon\.pty\.'         # PTY lifecycle events
grep 'daemon\.server\.'      # Server startup/shutdown
grep 'daemon\.connection\.'  # Client connection events

# tmux shim events
grep 'event":"shim\.'        # All shim events
grep 'shim\.split_window'    # Pane creation events
grep 'shim\.send_keys'       # Stdin write events
grep 'shim\.kill_pane'       # Pane destruction events
grep 'shim\.capture_pane'    # Scrollback capture events
grep 'shim\.ipc\.'           # IPC communication with daemon

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
    fn hide_window(&self, window_id: &str) -> Result<(), TerminalError>;
    fn close_window(&self, window_id: Option<&str>);
    fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError>;
}
```

Backends registered in `terminal/registry.rs`. Detection preference varies by platform:

- macOS: Ghostty > iTerm > Terminal.app
- Linux: Alacritty (requires Hyprland window manager)

Status detection uses PID tracking by default. Ghostty uses window-based detection as fallback when PID is unavailable. Alacritty on Linux uses Hyprland IPC for window management.

## tmux Shim for Agent Teams

**Purpose:** Makes Claude Code agent teams work transparently inside daemon-managed kild sessions.

**How it works:**

1. `kild create --daemon` sets `$TMUX` + `$TMUX_PANE` + prepends `~/.kild/bin` to `$PATH` in the PTY environment
2. Claude Code detects `$TMUX` and uses tmux pane backend for agent teams
3. `kild-core` symlinks `kild-tmux-shim` binary as `~/.kild/bin/tmux` during first daemon session creation
4. When Claude Code calls `tmux split-window`, `tmux send-keys`, etc., those calls hit the shim
5. Shim creates new daemon PTYs for teammates via IPC, manages pane state locally in `~/.kild/shim/<session>/`
6. `kild destroy` automatically cleans up all child shim PTYs

**Supported tmux commands:** `split-window` (creates daemon PTYs), `send-keys` (writes to PTY stdin with key name translation), `kill-pane` (destroys PTYs), `display-message` (expands format strings), `list-panes`, `select-pane`, `set-option`, `select-layout` (no-op), `resize-pane` (no-op), `has-session`, `new-session`, `new-window`, `list-windows`, `break-pane`, `join-pane`, `capture-pane` (reads PTY scrollback with `-p` for print, `-S` for start line).

**State management:** File-based pane registry at `~/.kild/shim/<session_id>/panes.json` with flock-based concurrency control. Each pane maps to a daemon session ID.

**Environment variables:**

- `$TMUX` - Set by kild-core, triggers Claude Code's tmux backend
- `$TMUX_PANE` - Current pane ID (e.g., `%0` for leader, `%1`, `%2` for teammates)
- `$KILD_SHIM_SESSION` - Session ID for shim state lookup
- `$KILD_SHIM_LOG` - Enable shim logging (file path or `1` for `~/.kild/shim/<session>/shim.log`)
- `$KILD_SESSION_BRANCH` - Branch name for Codex notify hook status reporting (fallback when `--self` PWD detection unavailable)

**Integration points in kild-core:**

- `daemon_helpers.rs:ensure_shim_binary()` - Symlinks shim as `~/.kild/bin/tmux` (best-effort, warns on failure)
- `daemon_helpers.rs:ensure_codex_notify_hook()` - Installs `~/.kild/hooks/codex-notify` for Codex CLI integration (idempotent, best-effort)
- `daemon_helpers.rs:ensure_codex_config()` - Patches `~/.codex/config.toml` with notify hook (respects existing config, best-effort)
- `daemon_helpers.rs:build_daemon_create_request()` - Injects shim and Codex env vars into daemon PTY requests
- `create.rs:create_session()` - Initializes shim state directory, `panes.json`, and Codex notify hook for daemon sessions
- `open.rs:open_session()` - Ensures Codex notify hook when opening Codex sessions
- `destroy.rs:destroy_session()` - Destroys child shim PTYs via daemon IPC, removes `~/.kild/shim/<session>/`, and cleans up task lists at `~/.claude/tasks/<task_list_id>/`

## Codex Notify Hook Integration

**Purpose:** Auto-configures Codex CLI to report agent activity states (idle/waiting) back to KILD via `agent-status` command.

**How it works:**

1. `kild create/open --agent codex` installs `~/.kild/hooks/codex-notify` shell script
2. Script is patched into `~/.codex/config.toml` as `notify = ["<path>"]`
3. Codex CLI calls the hook with JSON events on stdin (`agent-turn-complete`, `approval-requested`)
4. Hook maps events to KILD statuses: `agent-turn-complete` → idle, `approval-requested` → waiting
5. Hook calls `kild agent-status --self <status> --notify` to update session state and send desktop notifications

**Hook script:** `~/.kild/hooks/codex-notify` (shell script, auto-generated, do not edit)

**Config patching behavior:**

- Idempotent: runs on every `create/open --agent codex` but only patches if needed
- Respects existing user config: if `notify = [...]` is already set with a non-empty array, no changes are made
- Creates `~/.codex/config.toml` if missing
- Appends notify line if config exists but has missing or empty notify field

**Environment variables:**

- `$KILD_SESSION_BRANCH` - Injected into Codex sessions as fallback for `--self` PWD-based detection

**Best-effort pattern:** All operations warn on failure but never block session creation. If hook install or config patch fails, user sees warnings with manual remediation steps.

## Forge Backend Pattern

```rust
pub trait ForgeBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn is_pr_merged(&self, worktree_path: &Path, branch: &str) -> bool;
    fn check_pr_exists(&self, worktree_path: &Path, branch: &str) -> PrCheckResult;
    fn fetch_pr_info(&self, worktree_path: &Path, branch: &str) -> Option<PrInfo>;
}
```

Backends registered in `forge/registry.rs`. Forge detection via `detect_forge()` inspects git remote URL. Currently supports:

- GitHub (via `gh` CLI)
- Future: GitLab, Bitbucket, Gitea

Override auto-detection with `[git] forge = "github"` in config. PR types (PrInfo, PrState, CiStatus, ReviewStatus) defined in `forge/types.rs`.

## Configuration Hierarchy

Priority (highest wins): CLI args → project config (`./.kild/config.toml`) → user config (`~/.kild/config.toml`) → defaults

**Array Merging:** `include_patterns.patterns` arrays are merged (deduplicated) from user and project configs. Other config values follow standard override behavior.

**Daemon Runtime Config:** Control whether sessions run in daemon-owned PTYs or external terminals via `[daemon]` section:

```toml
[daemon]
enabled = false      # Use daemon mode by default for `kild create`
auto_start = true    # Auto-start daemon if not running
```

Runtime mode resolution for `kild create`:

1. `--daemon` flag → Daemon mode
2. `--no-daemon` flag → Terminal mode
3. Config `daemon.enabled = true` → Daemon mode
4. Default → Terminal mode

All sessions store their `runtime_mode` (Terminal or Daemon) in the session file. Sessions created with `--daemon` also store `daemon_session_id` in `AgentProcess`. Use `kild attach <branch>` to connect to daemon sessions (Ctrl+C to detach).

Runtime mode resolution for `kild open`:

1. `--daemon` flag → Daemon mode
2. `--no-daemon` flag → Terminal mode
3. Session's stored `runtime_mode` (auto-detect from creation) → Daemon or Terminal mode
4. Config `daemon.enabled = true` → Daemon mode
5. Default → Terminal mode

When using `open --all`, each kild respects its own stored runtime mode unless overridden by explicit flags (`--daemon` or `--no-daemon`). The CLI output shows `[daemon]` or `[terminal]` labels to indicate which mode each kild reopened in

**Daemon status:** The daemon runtime supports both foreground (`--foreground`) and background modes, auto-start via config, scrollback replay on attach, PTY exit notification with session state transitions, lazy status sync on `kild list`/`kild status` (daemon-managed sessions auto-update to Stopped when daemon reports exit), and `kild open` with daemon runtime mode.

**Agent teams:** Daemon sessions automatically inject `$TMUX` environment variable and configure the tmux shim (see "tmux Shim for Agent Teams" section) to enable Claude Code agent teams without external tmux installation.

## Error Handling

All domain errors implement `KildError` trait with `error_code()` and `is_user_error()`. Use `thiserror` for definitions.
