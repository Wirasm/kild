# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core Principles

**Target User:** Power users and agentic-forward engineers who want speed, control, and isolation. Users who run multiple AI agents simultaneously and need clean environment separation.

**Single-Developer Tool:** No multi-tenant complexity. Optimize for the solo developer managing parallel AI workflows.

**Type Safety (CRITICAL):** Rust's type system is a feature, not an obstacle. Use it fully.

## Codebase Realities

These facts should drive every design decision:

**Trait + factory architecture is the stability backbone.** Extension points are intentionally explicit and swappable. Most features should be added via trait implementation + factory registration, not cross-cutting rewrites.

**Performance and binary size are product goals, not nice-to-have.** Cargo.toml release profile and dependency choices optimize for size and determinism. Convenience dependencies and broad abstractions can silently regress these goals.

## Engineering Principles

These are mandatory implementation constraints, not slogans.

**KISS** — Prefer straightforward control flow over clever meta-programming. Prefer explicit match branches and typed structs over hidden dynamic behavior. Keep error paths obvious and localized.

**YAGNI** — Do not add new config keys, trait methods, feature flags, or workflow branches without a concrete accepted use case. Do not introduce speculative abstractions without at least one current caller. Keep unsupported paths explicit (error out) rather than adding partial fake support.

**DRY + Rule of Three** — Duplicate small, local logic when it preserves clarity. Extract shared utilities only after repeated, stable patterns (rule of three). When extracting, preserve module boundaries and avoid hidden coupling.

**SRP + ISP** — Keep each module focused on one concern. Extend behavior by implementing existing narrow traits whenever possible. Avoid fat interfaces and "god modules" that mix policy + transport + storage.

**Fail Fast + Explicit Errors** — Prefer explicit `bail!`/errors for unsupported or unsafe states. Never silently broaden permissions or capabilities. Document fallback behavior when fallback is intentional and safe.

**Determinism + Reproducibility** — Prefer reproducible commands and locked dependency behavior in CI-sensitive paths. Keep tests deterministic (no flaky timing/network dependence without guardrails). Ensure local validation commands map to CI expectations.

**Reversibility + Rollback-First** — Keep changes easy to revert (small scope, clear blast radius). For risky changes, define rollback path before merge. Avoid mixed mega-patches that block safe rollback.

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
cargo build -p kild-config     # Build config crate
cargo build -p kild-paths      # Build paths crate
cargo build -p kild-protocol   # Build protocol types crate

# Test
cargo test --all               # Run all tests
cargo test -p kild-core        # Test specific crate
cargo test -p kild-config      # Test config crate
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
cargo run -p kild -- create my-branch --main           # Run from project root (no worktree)
cargo run -p kild -- create my-branch --initial-prompt "Start with auth"  # Inject prompt on startup
cargo run -p kild -- list                              # List all kilds
cargo run -p kild -- list --json                       # JSON output
cargo run -p kild -- open my-branch                    # Reopen agent in existing kild
cargo run -p kild -- open --all                        # Open all stopped kilds
cargo run -p kild -- open my-branch --resume           # Resume previous conversation
cargo run -p kild -- open my-branch --no-attach        # Open daemon session without attach window
cargo run -p kild -- open my-branch --no-attach --resume  # Headless resume (brain reopening workers)
cargo run -p kild -- open my-branch --initial-prompt "Next task: ..."  # Inject prompt on reopen
cargo run -p kild -- inject my-branch "do the thing"  # Send to worker (inbox for claude, PTY for others)
cargo run -p kild -- inject my-branch "msg" --inbox   # Force Claude Code inbox protocol
cargo run -p kild -- inbox my-branch                   # Show fleet dropbox state for a session
cargo run -p kild -- inbox my-branch --json            # JSON output
cargo run -p kild -- inbox my-branch --task            # Show only task content
cargo run -p kild -- inbox my-branch --report          # Show only report content
cargo run -p kild -- inbox my-branch --status          # Show only ack status line
cargo run -p kild -- inbox --all                       # Show all fleet sessions
cargo run -p kild -- prime my-branch                   # Generate fleet context blob for agent bootstrapping
cargo run -p kild -- prime my-branch --json            # JSON output
cargo run -p kild -- prime my-branch --status          # Fleet status table only (compact)
cargo run -p kild -- prime --all                       # Concatenated prime blobs for all fleet sessions
cargo run -p kild -- prime --all --status              # Single deduplicated fleet table
cargo run -p kild -- prime --all --json                # JSON array of per-session prime contexts
cargo run -p kild -- stop my-branch                    # Stop agent, preserve kild
cargo run -p kild -- stop --all                        # Stop all kilds
cargo run -p kild -- stop my-branch --pane %1          # Stop a single teammate pane
cargo run -p kild -- attach my-branch --pane %1        # Attach to a specific teammate pane
cargo run -p kild -- teammates my-branch               # List all panes (leader + teammates)
cargo run -p kild -- teammates my-branch --json        # JSON output
cargo run -p kild -- complete my-branch                # Complete kild (PR cleanup)
```

**Full CLI reference:** Load the `/kild` skill for all commands (25+ commands including diff, stats, pr, rebase, sync, daemon, etc.). Load the `/kild-peek` skill for native app inspection commands (screenshots, UI interaction, assertions).

## Architecture

**Workspace structure:**

- `crates/kild-paths` - Centralized path construction for ~/.kild/ directory layout (KildPaths struct with typed methods for all paths including `tls_cert_path()` and `tls_key_path()` for daemon TLS certs, and `fleet_dir()`, `fleet_project_dir()`, `fleet_dropbox_dir()` for fleet dropbox paths). Single source of truth for KILD filesystem layout.
- `crates/kild-config` - TOML configuration types, loading, validation, and keybindings for ~/.kild/config.toml. Depends only on kild-paths and kild-protocol. Single source of truth for all KildConfig/Config/Keybindings types. Extracted from kild-core to enable fast incremental compilation of config-only changes.
- `crates/kild-protocol` - Shared IPC protocol types (ClientMessage, DaemonMessage, SessionInfo, SessionStatus, ErrorCode), domain newtypes (SessionId, BranchName, ProjectId), and serde-only domain enums (ForgeType). Also provides `IpcConnection` for JSONL-over-Unix-socket-or-TCP/TLS client used by both kild-core and kild-tmux-shim with connection health checking via `is_alive()` and TLS variant via `connect_tls()`, and `AsyncIpcClient<R, W>` — a generic async JSONL client over any `AsyncBufRead + AsyncWrite` pair used by kild-ui. All public enums are `#[non_exhaustive]` for forward compatibility. Newtypes defined via `newtype_string!` macro for compile-time type safety. Deps: serde, serde_json, futures (tempfile, smol for tests). No tokio, no kild-core. Single source of truth for daemon wire format and IPC client.
- `crates/kild-core` - Core library with all business logic, no CLI dependencies
- `crates/kild` - Thin CLI that consumes kild-core (clap for arg parsing, color.rs for Tallinn Night palette output)
- `crates/kild-daemon` - Standalone daemon binary for PTY management (async tokio server, JSONL IPC protocol, portable-pty integration). CLI spawns this as subprocess. Wire types re-exported from kild-protocol. Optionally binds a TLS-wrapped TCP listener (`bind_tcp`) alongside the Unix socket for remote access; self-signed cert auto-generated at `~/.kild/certs/` on first start.
- `crates/kild-tmux-shim` - tmux-compatible shim binary for agent team support (CLI that intercepts tmux commands, routes to daemon IPC via kild-protocol::IpcConnection)
- `crates/kild-teams` - Agent team discovery and state management library. Reads shim pane registries at `~/.kild/shim/` to enumerate leader + teammate panes and resolve their daemon session IDs. Used by CLI (`kild teammates`) and kild-ui (sidebar badge).
- `crates/kild-ui` - GPUI-based native GUI with multi-project support
- `crates/kild-peek-core` - Core library for native app inspection and interaction (window listing, screenshots, image comparison, assertions, UI automation)
- `crates/kild-peek` - CLI for visual verification of native macOS applications

**Key modules in kild-core:**

- `sessions/` - Session lifecycle (create, open, stop, destroy, complete, list). `fleet.rs` handles Honryū fleet mode — injecting team flags and managing inbox/config for claude daemon sessions. `dropbox.rs` manages per-session fleet dropbox directories at `~/.kild/fleet/<project_id>/<branch>/` including protocol generation, env var injection, cleanup, `read_dropbox_state()` for inspecting current protocol state, and `generate_prime_context()` for building full fleet context blobs (`FleetEntry`, `PrimeContext`) consumed by `kild prime`.
- `terminal/` - Multi-backend terminal abstraction (Ghostty, iTerm, Terminal.app, Alacritty)
- `agents/` - Agent backend system (amp, claude, kiro, gemini, codex, opencode, resume.rs for session continuity)
- `daemon/` - Daemon client for IPC communication with thread-local connection pooling (delegates to kild-protocol::IpcConnection) and auto-start logic (discovers kild-daemon binary as sibling executable). `tofu.rs` implements SHA-256 TOFU fingerprint verification for remote TCP/TLS connections. `mod.rs` exposes `set_remote_override()` for `--remote` CLI flag to route connections via TCP/TLS without touching handler signatures.
- `editor/` - Editor backend system (Zed, VS Code, Vim, generic fallback) with registry.rs for detection and resolution chain (CLI > config > $VISUAL > $EDITOR > OS default via duti/xdg-mime > PATH scan)
- `git/` - Git worktree operations via git2
- `forge/` - Forge backend system (GitHub, future: GitLab, Bitbucket, Gitea) for PR operations
- `config/` - REMOVED (moved to kild-config crate). kild-core re-exports all types from kild-config.
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
- `teams/` - TeamManager for resolving teammate counts per session (used by sidebar for [N] badge display)
- `views/` - GPUI components (permanent Rail | Sidebar | Main | StatusBar layout with project_rail.rs for 48px project switcher with settings gear, sidebar.rs for kild navigation grouped by Active/Stopped with nested terminal items, hover actions, and [N] teammate badge for active agent teams, ActiveView enum for Control/Dashboard/Detail tab bar, dashboard_view.rs for fleet overview cards, detail_view.rs for kild drill-down, terminal_tabs.rs for multi-terminal support, status_bar.rs for contextual alerts and keyboard hints, main_view/ for main view implementation)
- `terminal/` - Live terminal rendering with PTY integration (state.rs for PTY lifecycle with snapshot via `sync()`/`last_content()`, types.rs for `TerminalContent` snapshot type and `IndexedCell` alias, terminal_element/ for GPUI Element implementation, terminal_view.rs for View — calls `sync()` before constructing TerminalElement to minimize FairMutex hold time during prepaint, colors.rs for ANSI mapping, input.rs for keystroke translation)
- `daemon_client.rs` - Async daemon IPC client for GPUI. `ErasedUiClient` type erasure unifies Unix socket and TCP/TLS transports; `connect_for_config()` reads config to choose the right transport.
- `watcher.rs` - File system watcher for instant UI updates on session changes
- `refresh.rs` - Background refresh logic with hybrid file watching + slow poll fallback

**Key modules in kild-daemon:**

- `protocol/` - JSONL IPC protocol (ClientMessage, DaemonMessage, codec with flush/no-flush variants)
- `pty/` - PTY lifecycle management (PtyManager, ManagedPty via portable-pty, output broadcasting)
- `session/` - Daemon session state machine (SessionManager, DaemonSession, SessionState enum)
- `server/` - Unix socket server with optional TCP/TLS listener (async connection handling, message dispatch, signal-based shutdown; `handle_connection<S>` is generic over stream type)
- `tls.rs` - TLS cert generation and loading (self-signed cert auto-generated at `~/.kild/certs/` on first `bind_tcp` start)
- `client/` - Daemon client for typed IPC operations (DaemonClient)

**Key modules in kild-peek-core:**

- `window/` - Window and monitor enumeration via macOS APIs (handler/ contains builders.rs, find.rs, list.rs, monitors.rs, tests.rs)
- `screenshot/` - Screenshot capture with multiple targets (window, monitor, base64 output)
- `diff/` - Image comparison using SSIM algorithm
- `assert/` - UI state assertions (window exists, visible, image similarity, element text presence)
- `interact/` - Native UI interaction (handler/ contains click.rs, helpers.rs, keyboard.rs, mouse.rs, tests.rs)
- `element/` - Accessibility API-based element enumeration, text search, element finding, and wait for element to appear/disappear
- `logging/` - Tracing initialization matching kild-core patterns
- `events/` - App lifecycle event helpers

**Key modules in kild-tmux-shim:**

- `parser/` - Hand-rolled tmux argument parser for ~15 subcommands + aliases (parse.rs, types.rs, tests.rs)
- `commands.rs` - Command handlers dispatching to daemon IPC or local state
- `state.rs` - File-based pane registry with flock concurrency control
- `ipc.rs` - Domain-specific IPC helpers with thread-local connection pooling (delegates to kild-protocol::IpcConnection)
- `main.rs` - Entry point, file-based logging controlled by KILD_SHIM_LOG env var
- `errors.rs` - ShimError type

**Key modules in kild-teams:**

- `discovery.rs` - Fallback teammate discovery from shim pane registry (leader + teammates from `panes.json`)
- `parser.rs` - JSON parsing for shim pane registry format
- `types.rs` - Domain types: `TeamMember`, `TeamState`, `TeamColor`, `TeamEvent`
- `watcher.rs` - `TeamWatcher` for file-based watching of team state changes
- `scanner.rs` - Scans all sessions for active team state
- `mapper.rs` - Maps shim pane entries to `TeamMember` domain types
- `errors.rs` - `TeamsError` type

**Key modules in kild (CLI):**

- `app/` - CLI command implementations (daemon.rs, git.rs, global.rs, misc.rs, project.rs, query.rs, session.rs, tests.rs). `global.rs` parses `--remote`/`--remote-fingerprint` flags and calls `set_remote_override()` to route all IPC over TCP/TLS for that invocation.
- `commands/` - Individual command handler modules (teammates.rs, stop.rs, attach.rs, inject.rs, inbox.rs, prime.rs, and others)
- `main.rs` - CLI entry point with clap argument parsing
- `color.rs` - Tallinn Night palette output formatting

**Key modules in kild-peek (CLI):**

- `app/` - CLI app logic (assert.rs, diff.rs, elements.rs, interact.rs, list.rs, screenshot.rs, tests.rs)
- `commands/` - Command implementations (assert.rs, diff.rs, elements.rs, interact.rs, list.rs, screenshot.rs, window_resolution.rs)
- `main.rs` - CLI entry point

**Module pattern:** Each domain in kild-core starts with `errors.rs`, `types/`, `mod.rs`. Core types and submodules may be organized as directories (e.g., `sessions/types/` contains agent_process.rs, request.rs, safety.rs, session.rs, status.rs, tests.rs; `sessions/persistence/` contains patching.rs, session_files.rs, sidecar.rs, tests.rs; `sessions/integrations/` contains claude.rs, codex.rs, opencode.rs, mod.rs for agent hook + settings integration). Additional files vary by domain (e.g., `create.rs`/`open.rs`/`stop.rs`/`list.rs`/`destroy.rs`/`complete.rs`/`agent_status.rs`/`shim_setup.rs`/`attach.rs`/`daemon_request.rs` for sessions with `handler.rs` as re-export facade). kild-daemon uses a flatter structure with top-level errors/types and module-specific implementation files. kild-tmux-shim, kild (CLI), and kild-peek (CLI) use focused modules organized by domain (parser/, app/, commands/).

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

## Code Naming Contract

Apply these naming rules for all code changes unless a subsystem has a stronger existing pattern.

- Use Rust standard casing: modules/files `snake_case`, types/traits/enums `PascalCase`, functions/variables `snake_case`, constants/statics `SCREAMING_SNAKE_CASE`.
- Name types and modules by domain role, not implementation detail — e.g. `GhForgeBackend`, `GhosttyTerminalBackend` over vague names like `Manager` or `Helper`.
- Keep trait implementer naming explicit and predictable: `<Name>Backend`, `<Name>Agent`, `<Name>Channel`.
- Keep factory registration keys stable, lowercase, and user-facing (e.g. `"ghostty"`, `"claude"`, `"github"`). Avoid alias sprawl.
- Name tests by behavior/outcome: `<subject>_<expected_behavior>`. Keep fixture identifiers KILD-native (`kild_branch`, `kild_session`, `kild_project`).

## Architecture Boundary Contract

Keep the trait/factory architecture stable under growth.

- Extend capabilities by adding trait implementations + factory wiring first; avoid cross-module rewrites for isolated features.
- Keep dependency direction inward to contracts: concrete implementations depend on trait/config/util layers, not on each other.
- Avoid cross-subsystem coupling — e.g. terminal backend code must not import session internals, agent code must not mutate forge policy directly.
- Keep module responsibilities single-purpose: session lifecycle in `sessions/`, terminal abstraction in `terminal/`, agent backends in `agents/`, git ops in `git/`, forge ops in `forge/`.
- Introduce new shared abstractions only after repeated use (rule of three), with at least one real caller in current scope.
- Treat config/schema keys as public contract: document defaults, compatibility impact, and rollback path for any changes.

## Structured Logging

JSON output via tracing-subscriber. Silent by default — use `-v/--verbose` to emit `info` and above. `RUST_LOG` alone does not override quiet mode.

**Event format:** `{layer}.{domain}.{action}_{state}` — e.g. `core.session.create_started`, `cli.create_failed`, `daemon.pty.spawn_completed`. Layers map to crates (`cli`, `core`, `daemon`, `shim`, `ui`, `peek.cli`, `peek.core`). State suffixes: `_started`, `_completed`, `_failed`, `_skipped`.

**Why:** Logging serves four purposes: (1) pinpoint exactly where in the codebase something lives — the event name is a direct map to layer + domain + operation; (2) surface failures — never swallow errors silently, always emit `_failed` with the error attached; (3) match GitHub issue labels — domain names align with issue labels so a bug report on `core.session` maps directly to `crates/kild-core/sessions/`; (4) enable AI grep — consistent, predictable event names mean an AI agent can grep logs and immediately locate the relevant code. Every user-visible operation needs a `_started`/`_completed` pair. Each layer logs only its own concern.

**Field conventions:** Use `%e` (Display) for errors, `?val` (Debug) for enums/structs. Always name fields — never log bare `{:?}`.

```rust
info!(event = "cli.create_started", branch = branch, agent = config.agent.default);
error!(event = "cli.create_failed", error = %e);
info!(event = "core.git.worktree.create_completed", path = %worktree_path.display());
// Filter: grep '_failed"' or grep 'core\.session\.'
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

**Supported tmux commands:** `split-window` (creates daemon PTYs; supports shell-command after `--` or as trailing positional args — when provided, the command runs directly in the PTY and `#{pane_dead}` becomes `1` on exit), `send-keys` (writes to PTY stdin with key name translation), `kill-pane` (destroys PTYs), `display-message` (expands format strings), `list-panes`, `select-pane`, `set-option`, `select-layout` (no-op), `resize-pane` (no-op), `has-session`, `new-session`, `new-window`, `list-windows`, `break-pane`, `join-pane`, `capture-pane` (reads PTY scrollback with `-p` for print, `-S` for start line).

**Format variables:** `#{pane_id}`, `#{session_name}`, `#{window_index}`, `#{window_name}`, `#{pane_title}`, `#{pane_dead}` (queries daemon — `1` if process exited, `0` if running), `#{pane_pid}` (queries daemon — PID of pane process), `#{pane_dead_status}` (queries daemon — exit code of dead pane).

**State management:** File-based pane registry at `~/.kild/shim/<session_id>/panes.json` with flock-based concurrency control. Each pane maps to a daemon session ID.

**Environment variables:**

- `$TMUX` - Set by kild-core, triggers Claude Code's tmux backend
- `$TMUX_PANE` - Current pane ID (e.g., `%0` for leader, `%1`, `%2` for teammates)
- `$KILD_SHIM_SESSION` - Session ID for shim state lookup
- `$KILD_SHIM_LOG` - Enable shim logging (file path or `1` for `~/.kild/shim/<session>/shim.log`)
- `$KILD_SESSION_BRANCH` - Branch name for Claude Code and Codex notify hook status reporting (fallback when `--self` PWD detection unavailable)

**Integration points in kild-core:**

- `shim_setup.rs:ensure_shim_binary()` - Symlinks shim as `~/.kild/bin/tmux` (best-effort, warns on failure)
- `integrations/codex.rs:ensure_codex_notify_hook()` - Installs `~/.kild/hooks/codex-notify` for Codex CLI integration (idempotent, best-effort)
- `integrations/codex.rs:ensure_codex_config()` - Patches `~/.codex/config.toml` with notify hook (respects existing config, best-effort)
- `integrations/claude.rs:ensure_claude_status_hook()` - Installs `~/.kild/hooks/claude-status` for Claude Code integration (idempotent, best-effort)
- `integrations/claude.rs:ensure_claude_settings()` - Patches `~/.claude/settings.json` with hook entries (respects existing config, best-effort)
- `daemon_request.rs:build_daemon_create_request()` - Injects shim, Codex, Claude Code env vars, and fleet agent flags into daemon PTY requests
- `create.rs:create_session()` - Initializes shim state directory, `panes.json`, agent-specific hooks, fleet membership, and dropbox directory for daemon sessions
- `open.rs:open_session()` - Ensures agent-specific hooks, fleet membership, and dropbox directory when opening sessions
- `destroy.rs:destroy_session()` - Destroys child shim PTYs and UI-created daemon sessions via daemon IPC, removes `~/.kild/shim/<session>/`, cleans up task lists at `~/.claude/tasks/<task_list_id>/`, and removes fleet dropbox at `~/.kild/fleet/<project_id>/<branch>/`

## Agent Hook Integration

### Claude Code Status Hook Integration

**Purpose:** Auto-configures Claude Code to report agent activity states (idle/waiting) back to KILD via `agent-status` command.

**How it works:**

1. `kild create/open --agent claude` installs `~/.kild/hooks/claude-status` shell script
2. Script is patched into `~/.claude/settings.json` for Stop, Notification, SubagentStop, TeammateIdle, and TaskCompleted hook events
3. Claude Code calls the hook with JSON events on stdin
4. Hook maps events to KILD statuses: Stop/SubagentStop/TeammateIdle/TaskCompleted → idle, Notification(permission_prompt) → waiting, Notification(idle_prompt) → idle
5. Hook calls `kild agent-status --self <status> --notify` to update session state and send desktop notifications
6. On Stop/idle events, hook also calls `kild inject honryu "[EVENT] <branch> <event>: <last_message>"` to forward worker state to the brain session (skipped if the session IS honryu, preventing self-loops; also skipped if honryu is not running)

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

### Fleet Mode (Honryū)

**Purpose:** Auto-wires Claude Code agent team flags into daemon sessions so `kild inject` can deliver messages via the Claude Code inbox polling protocol.

**How it works:**

- Fleet mode activates when `~/.claude/teams/honryu/` exists or when creating the `honryu` (brain) session itself
- Daemon sessions with the `claude` agent get `--agent-id <branch>@honryu --agent-name <branch> --team-name honryu` appended to the agent command
- The brain session (`honryu` branch) additionally loads `--agent kild-brain` as team lead
- `kild inject <branch> "<text>"` routes via PTY stdin for non-claude agents; for claude sessions it writes to `~/.claude/teams/honryu/inboxes/<branch>.json` (Claude Code delivers it as a new user turn within ~1s). Use `--inbox` to force the inbox path.
- `ensure_fleet_member()` in `fleet.rs` creates the inbox file and team config on every create/open (idempotent, best-effort)
- `ensure_dropbox()` in `dropbox.rs` creates `~/.kild/fleet/<project_id>/<branch>/` with a `protocol.md` on every create/open (idempotent, best-effort). Directory is removed on destroy.
- Bare shell sessions are unaffected — they have no agent to consume tasks. Non-claude agents participate in the dropbox protocol but do not receive Claude Code inbox/team flags.

**Environment variables injected into fleet daemon sessions:**

- `$KILD_DROPBOX` - Path to the session's dropbox directory (`~/.kild/fleet/<project_id>/<branch>/`)
- `$KILD_FLEET_DIR` - Path to the project fleet directory (`~/.kild/fleet/<project_id>/`). Brain session only.

**Dropbox file protocol** (used by workers to communicate with the brain):

- `task-id` - Monotonically incrementing task counter (written by brain)
- `task.md` - Current task (written by brain)
- `ack` - Task acknowledgment: worker writes the task-id after reading task.md
- `report.md` - Task result (written by worker on completion)
- `history.jsonl` - Append-only audit trail of all injections (written by KILD)
- `protocol.md` - Protocol instructions (auto-generated by KILD, do not edit)

Inspect dropbox state with `kild inbox <branch>` (or `--all` for all fleet sessions). Generate a full fleet context blob for agent bootstrapping with `kild prime <branch>` — outputs protocol, current task, and fleet status as a composable markdown blob suitable for `kild inject worker "$(kild prime worker)"`. Use `kild prime --all` to get concatenated blobs for all fleet sessions, `--all --status` for a single deduplicated fleet table, or `--all --json` for a JSON array.

**Key files:** `crates/kild-core/src/sessions/fleet.rs`, `crates/kild-core/src/sessions/dropbox.rs`, `crates/kild/src/commands/inject.rs`, `crates/kild/src/commands/inbox.rs`, `crates/kild/src/commands/prime.rs`

**Typical brain setup:**

```
kild create honryu --daemon --main   # brain: runs from project root, no worktree
kild create <worker> --daemon        # worker: auto-joins fleet with team flags
kild inject <worker> "do the thing"  # brain → worker message
```

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

**Keybindings** use a separate file: project (`./.kild/keybindings.toml`) overrides user (`~/.kild/keybindings.toml`). Invalid bindings warn and fall back to defaults — never block startup. See `crates/kild-config/src/keybindings.rs` for the full schema.

**Array Merging:** `include_patterns.patterns` arrays are merged (deduplicated) from user and project configs. Other config values follow standard override behavior.

**Runtime mode resolution:** Sessions run in either daemon-owned PTYs or external terminals. Resolution order for both `create` and `open`: `--daemon`/`--no-daemon` flag → session's stored `runtime_mode` (open only) → config `daemon.enabled` → default (terminal). All sessions store their `runtime_mode` in the session file. Daemon sessions auto-open an attach window; use `kild attach <branch>` to reconnect.

**Agent teams:** Daemon sessions inject `$TMUX` and configure the tmux shim (see "tmux Shim for Agent Teams" section) to enable Claude Code agent teams without external tmux.

**UI keyboard shortcuts:**
All UI shortcuts are configurable via `~/.kild/keybindings.toml` (user) or `./.kild/keybindings.toml` (project). See `crates/kild-config/src/keybindings.rs` for all available keys and defaults.

## Error Handling

All domain errors implement `KildError` trait with `error_code()` and `is_user_error()`. Use `thiserror` for definitions.
