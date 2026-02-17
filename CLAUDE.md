# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core Principles

**Target User:** Power users and agentic-forward engineers who want speed, control, and isolation. Users who run multiple AI agents simultaneously and need clean environment separation.

**Single-Developer Tool:** No multi-tenant complexity. Optimize for the solo developer managing parallel AI workflows.

**KISS:** Keep it simple and easily understandable over complex and "clever". First principles thinking.

**YAGNI:** Justify every line of code against the problem it solves. Is it needed?

**DRY:** Don't Repeat Yourself. Every piece of knowledge should have a single, authoritative representation. Extract shared logic rather than copy-pasting.

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
cargo build -p kild-paths      # Build paths crate
cargo build -p kild-protocol   # Build protocol types crate

# Test
cargo test --all               # Run all tests
cargo test -p kild-core        # Test specific crate
cargo test -p kild-paths       # Test paths crate
cargo test -p kild-protocol    # Test protocol types crate
cargo test test_name           # Run single test by name

# Lint & Format
cargo fmt                      # Format code
cargo fmt --check              # Check formatting
cargo clippy --all -- -D warnings  # Lint with warnings as errors

# Run (essential commands — full reference in /kild and /kild-peek skills)
cargo run -p kild -- create my-branch                  # Create kild with default agent
cargo run -p kild -- create my-branch --note "Auth"    # Create with description
cargo run -p kild -- create my-branch --yolo           # Create with autonomous mode
cargo run -p kild -- list                              # List all kilds
cargo run -p kild -- list --json                       # JSON output
cargo run -p kild -- open my-branch                    # Reopen agent in existing kild
cargo run -p kild -- open --all                        # Open all stopped kilds
cargo run -p kild -- open my-branch --resume           # Resume previous conversation
cargo run -p kild -- stop my-branch                    # Stop agent, preserve kild
cargo run -p kild -- stop --all                        # Stop all kilds
cargo run -p kild -- complete my-branch                # Complete kild (PR cleanup)
```

**Full CLI reference:** Load the `/kild` skill for all commands (25+ commands including diff, stats, pr, rebase, sync, daemon, etc.). Load the `/kild-peek` skill for native app inspection commands (screenshots, UI interaction, assertions).

## Architecture

**Workspace structure:**
- `crates/kild-paths` - Centralized path construction for ~/.kild/ directory layout (KildPaths struct with typed methods for all paths). Single source of truth for KILD filesystem layout.
- `crates/kild-protocol` - Shared IPC protocol types (ClientMessage, DaemonMessage, SessionInfo, SessionStatus, ErrorCode) and domain newtypes (SessionId, BranchName, ProjectId). Also provides `IpcConnection` for JSONL-over-Unix-socket client used by both kild-core and kild-tmux-shim with connection health checking via `is_alive()`. All public enums are `#[non_exhaustive]` for forward compatibility. Newtypes defined via `newtype_string!` macro for compile-time type safety. Deps: serde, serde_json only (tempfile for tests). No tokio, no kild-core. Single source of truth for daemon wire format and IPC client.
- `crates/kild-core` - Core library with all business logic, no CLI dependencies
- `crates/kild` - Thin CLI that consumes kild-core (clap for arg parsing, color.rs for Tallinn Night palette output)
- `crates/kild-daemon` - Standalone daemon binary for PTY management (async tokio server, JSONL IPC protocol, portable-pty integration). CLI spawns this as subprocess. Wire types re-exported from kild-protocol.
- `crates/kild-tmux-shim` - tmux-compatible shim binary for agent team support (CLI that intercepts tmux commands, routes to daemon IPC via kild-protocol::IpcConnection)
- `crates/kild-ui` - GPUI-based native GUI with multi-project support
- `crates/kild-peek-core` - Core library for native app inspection and interaction (window listing, screenshots, image comparison, assertions, UI automation)
- `crates/kild-peek` - CLI for visual verification of native macOS applications

**Key modules in kild-core:**
- `sessions/` - Session lifecycle (create, open, stop, destroy, complete, list)
- `terminal/` - Multi-backend terminal abstraction (Ghostty, iTerm, Terminal.app, Alacritty)
- `agents/` - Agent backend system (amp, claude, kiro, gemini, codex, opencode, resume.rs for session continuity)
- `daemon/` - Daemon client for IPC communication with thread-local connection pooling (delegates to kild-protocol::IpcConnection) and auto-start logic (discovers kild-daemon binary as sibling executable)
- `editor/` - Editor backend system (Zed, VS Code, Vim, generic fallback) with registry.rs for detection and resolution chain (CLI > config > $VISUAL > $EDITOR > OS default via duti/xdg-mime > PATH scan)
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
- `state/` - Type-safe state modules with encapsulated AppState facade (app_state/ for state and tests, dialog.rs, errors.rs, loading.rs, selection.rs, sessions.rs)
- `actions.rs` - User actions (create, open, stop, destroy, project management)
- `views/` - GPUI components (permanent Rail | Sidebar | Main | StatusBar layout with project_rail.rs for 48px project switcher with settings gear, sidebar.rs for kild navigation grouped by Active/Stopped with nested terminal items and hover actions, ActiveView enum for Control/Dashboard/Detail tab bar, dashboard_view.rs for fleet overview cards, detail_view.rs for kild drill-down, terminal_tabs.rs for multi-terminal support, status_bar.rs for contextual alerts and keyboard hints, main_view/ for main view implementation)
- `terminal/` - Live terminal rendering with PTY integration (state.rs for PTY lifecycle, terminal_element/ for GPUI Element implementation, terminal_view.rs for View, colors.rs for ANSI mapping, input.rs for keystroke translation)
- `watcher.rs` - File system watcher for instant UI updates on session changes
- `refresh.rs` - Background refresh logic with hybrid file watching + slow poll fallback

**Key modules in kild-daemon:**
- `protocol/` - JSONL IPC protocol (ClientMessage, DaemonMessage, codec with flush/no-flush variants)
- `pty/` - PTY lifecycle management (PtyManager, ManagedPty via portable-pty, output broadcasting)
- `session/` - Daemon session state machine (SessionManager, DaemonSession, SessionState enum)
- `server/` - Unix socket server (async connection handling, message dispatch, signal-based shutdown)
- `client/` - Daemon client for typed IPC operations (DaemonClient)

**Key modules in kild-peek-core:**
- `window/` - Window and monitor enumeration via macOS APIs (handler/ contains builders.rs, find.rs, list.rs, monitors.rs, tests.rs)
- `screenshot/` - Screenshot capture with multiple targets (window, monitor, base64 output)
- `diff/` - Image comparison using SSIM algorithm
- `assert/` - UI state assertions (window exists, visible, image similarity)
- `interact/` - Native UI interaction (handler/ contains click.rs, helpers.rs, keyboard.rs, mouse.rs, tests.rs)
- `element/` - Accessibility API-based element enumeration, text search, and element finding
- `logging/` - Tracing initialization matching kild-core patterns
- `events/` - App lifecycle event helpers

**Key modules in kild-tmux-shim:**
- `parser/` - Hand-rolled tmux argument parser for ~15 subcommands + aliases (parse.rs, types.rs, tests.rs)
- `commands.rs` - Command handlers dispatching to daemon IPC or local state
- `state.rs` - File-based pane registry with flock concurrency control
- `ipc.rs` - Domain-specific IPC helpers with thread-local connection pooling (delegates to kild-protocol::IpcConnection)
- `main.rs` - Entry point, file-based logging controlled by KILD_SHIM_LOG env var
- `errors.rs` - ShimError type

**Key modules in kild (CLI):**
- `app/` - CLI command implementations (daemon.rs, git.rs, global.rs, misc.rs, query.rs, session.rs, tests.rs)
- `main.rs` - CLI entry point with clap argument parsing
- `color.rs` - Tallinn Night palette output formatting

**Key modules in kild-peek (CLI):**
- `app/` - CLI app logic (assert.rs, diff.rs, elements.rs, interact.rs, list.rs, screenshot.rs, tests.rs)
- `commands/` - Command implementations (assert.rs, diff.rs, elements.rs, interact.rs, list.rs, screenshot.rs, window_resolution.rs)
- `main.rs` - CLI entry point

**Module pattern:** Each domain in kild-core starts with `errors.rs`, `types/`, `mod.rs`. Core types and submodules may be organized as directories (e.g., `sessions/types/` contains agent_process.rs, request.rs, safety.rs, session.rs, status.rs, tests.rs; `sessions/persistence/` contains patching.rs, session_files.rs, sidecar.rs, tests.rs). Additional files vary by domain (e.g., `create.rs`/`open.rs`/`stop.rs`/`list.rs`/`destroy.rs`/`complete.rs`/`agent_status.rs`/`daemon_helpers.rs` for sessions with `handler.rs` as re-export facade). kild-daemon uses a flatter structure with top-level errors/types and module-specific implementation files. kild-tmux-shim, kild (CLI), and kild-peek (CLI) use focused modules organized by domain (parser/, app/, commands/).

**CLI interaction:** Commands delegate directly to `kild-core` handlers. No business logic in CLI layer.

**Command pattern:** Business operations are defined as `Command` enum variants in `kild-core/state/types.rs`. The `Store` trait in `kild-core/state/store.rs` provides the dispatch contract, returning `Vec<Event>` on success to describe state changes. The `Event` enum in `kild-core/state/events.rs` defines all business state changes (kild lifecycle, project management).

**Dispatch vs direct-call guidance:**
- **UI state-mutating operations**: Always dispatch through Store. All operations use `Command` → `CoreStore::dispatch` → `Event` → `apply_events`. This ensures consistent event-driven state updates.
- **CLI operations**: Call handler functions directly (e.g., `session_ops::create_session`). The CLI is synchronous and doesn't need event-driven updates.
- **Read-only queries**: Call handler functions directly from both CLI and UI. Queries like `list_sessions`, `get_session`, `get_destroy_safety_info` don't mutate state and don't need dispatch.

## Code Style Preferences

**Prefer string literals over enums for event names.** Event names are typed as string literals directly in the tracing macros, not as enum variants. This keeps logging flexible and greppable.

**Use newtypes for domain identifiers.** Session IDs, branch names, and project IDs use the `newtype_string!` macro pattern for compile-time type safety. See `kild-protocol/src/types.rs` for the macro and existing newtypes. This prevents mixing up identifiers at the type level.

**No backward-compatibility shims.** When renaming or moving types, rename all usages. Never introduce type aliases, re-exports, or wrapper types solely for backward compatibility. This is a single-developer tool with no external consumers — there is nobody to keep compatible with. One name, one type, everywhere.

**No `.unwrap()` / `.expect()` in production code.** Always propagate errors with `?` or handle them explicitly. Panicking crashes the process — use `Result` for all fallible operations.

**Don't clone to satisfy the borrow checker.** If the borrow checker complains, understand the ownership issue first. Restructure borrows, use `mem::take`, or redesign the data flow. Cloning `Rc`/`Arc` is fine — that's their purpose.

**Prefer borrowed types for function arguments.** Use `&str` over `&String`, `&[T]` over `&Vec<T>`, `&Path` over `&PathBuf`. This accepts more input types via deref coercion and avoids unnecessary indirection.

## Structured Logging

### Setup

Logging is initialized via `kild_core::init_logging(quiet)` in the CLI main.rs. Output is JSON format via tracing-subscriber.

By default, all log output is suppressed (clean output). When `-v/--verbose` flag is used, info-level and above events are emitted.

The `-v/--verbose` flag is required to see any logs. The `RUST_LOG` env var alone will not override the default quiet mode.

Enable verbose logs with the verbose flag: `cargo run -- -v list`

### Event Naming Convention

All events follow: `{layer}.{domain}.{action}_{state}`

| Layer | Crate | Description |
|-------|-------|-------------|
| `cli` | `crates/kild/` | User-facing CLI commands |
| `core` | `crates/kild-core/` | Core library logic |
| `daemon` | `crates/kild-daemon/` | Daemon server and PTY management |
| `shim` | `crates/kild-tmux-shim/` | tmux shim binary operations |
| `ui` | `crates/kild-ui/` | GPUI native GUI |
| `peek.cli` | `crates/kild-peek/` | kild-peek CLI commands |
| `peek.core` | `crates/kild-peek-core/` | kild-peek core library |

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

// Claude Code status hook integration
info!(event = "core.session.claude_status_hook_installed", path = %hook_path.display());
debug!(event = "core.session.claude_status_hook_already_exists", path = %hook_path.display());
warn!(event = "core.session.claude_status_hook_failed", error = %msg);
info!(event = "core.session.claude_settings_patched", path = %settings_path.display());
info!(event = "core.session.claude_settings_already_configured");
warn!(event = "core.session.claude_settings_patch_failed", error = %msg);

// Codex notify hook integration
info!(event = "core.session.codex_notify_hook_installed", path = %hook_path.display());
warn!(event = "core.session.codex_notify_hook_failed", error = %msg);
info!(event = "core.session.codex_config_patched", path = %config_path.display());
info!(event = "core.session.codex_config_already_configured");
warn!(event = "core.session.codex_config_patch_failed", error = %msg);

// Auto-attach for daemon sessions
info!(event = "core.session.auto_attach_started", branch = branch);
info!(event = "core.session.auto_attach_completed", branch = branch, window_id = ?window_id);
warn!(event = "core.session.auto_attach_failed", branch = branch, error = %e);

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
- `$KILD_SESSION_BRANCH` - Branch name for Claude Code and Codex notify hook status reporting (fallback when `--self` PWD detection unavailable)

**Integration points in kild-core:**
- `daemon_helpers.rs:ensure_shim_binary()` - Symlinks shim as `~/.kild/bin/tmux` (best-effort, warns on failure)
- `daemon_helpers.rs:ensure_codex_notify_hook()` - Installs `~/.kild/hooks/codex-notify` for Codex CLI integration (idempotent, best-effort)
- `daemon_helpers.rs:ensure_codex_config()` - Patches `~/.codex/config.toml` with notify hook (respects existing config, best-effort)
- `daemon_helpers.rs:ensure_claude_status_hook()` - Installs `~/.kild/hooks/claude-status` for Claude Code integration (idempotent, best-effort)
- `daemon_helpers.rs:ensure_claude_settings()` - Patches `~/.claude/settings.json` with hook entries (respects existing config, best-effort)
- `daemon_helpers.rs:build_daemon_create_request()` - Injects shim, Codex, and Claude Code env vars into daemon PTY requests
- `create.rs:create_session()` - Initializes shim state directory, `panes.json`, and agent-specific hooks for daemon sessions
- `open.rs:open_session()` - Ensures agent-specific hooks when opening sessions
- `destroy.rs:destroy_session()` - Destroys child shim PTYs and UI-created daemon sessions via daemon IPC, removes `~/.kild/shim/<session>/`, and cleans up task lists at `~/.claude/tasks/<task_list_id>/`

## Agent Hook Integration

### Claude Code Status Hook Integration

**Purpose:** Auto-configures Claude Code to report agent activity states (idle/waiting) back to KILD via `agent-status` command.

**How it works:**

1. `kild create/open --agent claude` installs `~/.kild/hooks/claude-status` shell script
2. Script is patched into `~/.claude/settings.json` for Stop, Notification, SubagentStop, TeammateIdle, and TaskCompleted hook events
3. Claude Code calls the hook with JSON events on stdin
4. Hook maps events to KILD statuses: Stop/SubagentStop/TeammateIdle/TaskCompleted → idle, Notification(permission_prompt) → waiting, Notification(idle_prompt) → idle
5. Hook calls `kild agent-status --self <status> --notify` to update session state and send desktop notifications

**Hook script:** `~/.kild/hooks/claude-status` (shell script, auto-generated, do not edit)

**Settings patching behavior:**
- Idempotent: runs on every `create/open --agent claude` but only patches if needed
- Respects existing user hooks: if any hook event already references the claude-status script, skips patching
- Creates `~/.claude/settings.json` if missing
- Preserves all existing settings and hooks

**Environment variables:**
- `$KILD_SESSION_BRANCH` - Injected into Claude Code sessions as fallback for `--self` PWD-based detection

**Manual setup:** Run `kild init-hooks claude` to install hook and patch settings without creating a session.

**Best-effort pattern:** All operations warn on failure but never block session creation. If hook install or settings patch fails, user sees warnings with manual remediation steps.

### Codex Notify Hook Integration

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

**All config options are documented in `.kild/config.example.toml`.** Load the `/kild` skill for help with config changes.

**Array Merging:** `include_patterns.patterns` arrays are merged (deduplicated) from user and project configs. Other config values follow standard override behavior.

**Runtime mode resolution:** Sessions run in either daemon-owned PTYs or external terminals. Resolution order for both `create` and `open`: `--daemon`/`--no-daemon` flag → session's stored `runtime_mode` (open only) → config `daemon.enabled` → default (terminal). All sessions store their `runtime_mode` in the session file. Daemon sessions auto-open an attach window; use `kild attach <branch>` to reconnect.

**Agent teams:** Daemon sessions inject `$TMUX` and configure the tmux shim (see "tmux Shim for Agent Teams" section) to enable Claude Code agent teams without external tmux.

**UI keyboard shortcuts:**
- `<nav_modifier>+1-9`: Jump to Nth kild in sidebar (modifier configured by `[ui] nav_modifier`, default: "ctrl")
- `Cmd+Shift+[/]`: Cycle between Control workspaces
- `Cmd+J/K`: Navigate next/previous kild
- `Cmd+D`: Toggle Control/Dashboard view
- `Ctrl+Escape`: Move focus from terminal to sidebar

## Error Handling

All domain errors implement `KildError` trait with `error_code()` and `is_user_error()`. Use `thiserror` for definitions.
