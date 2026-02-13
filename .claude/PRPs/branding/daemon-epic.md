# KILD Daemon Epic

*February 2026*

KILD today is a production CLI + GPUI dashboard managing parallel AI agents in external terminal windows. The daemon vision transforms KILD into the runtime itself — owning PTYs, embedding terminals, replacing tmux. This epic tracks that transformation.

**Research:** [daemon-vision-research.md](../branding/daemon-vision-research.md) — full technical research, architecture decisions, competitive analysis, and open questions.

**Brand/UX:** [mockup-v2.html](../branding/mockup-v2.html) — the target multiplexer UI (three-view architecture). [brand-system.html](../branding/brand-system.html) — Tallinn Night design system.

**Vision:** [VISION.md](../branding/VISION.md) — mission, market positioning, expansion path.

---

## Where We Are

| Crate | Lines | State |
|-------|-------|-------|
| kild-core | 28.6k | Production. Sessions, git worktrees, terminal backends, config, forge, health, daemon client, notifications, session resume. |
| kild (CLI) | 6k | Production. 20+ commands, thin delegation to kild-core. Daemon subcommands + attach. |
| kild-daemon | 4k | Production. PTY ownership via portable-pty, JSONL IPC, session state machine, async tokio server, scrollback replay, PTY exit handling. 15 integration tests. |
| kild-tmux-shim | 3k | Production. 16 tmux commands, hand-rolled parser, file-based pane registry, daemon IPC client. 90 unit tests. |
| kild-ui | ~12k | Working. GPUI 0.2.2 (crates.io), terminal multiplexer with per-kild terminals, tab bar, daemon-backed terminals via alacritty_terminal + portable-pty + daemon IPC. Phase 2.4 complete. |
| kild-peek/core | 13k | Production. macOS window inspection, UI automation. |

**What works:** Git worktree isolation, session lifecycle, multi-agent tracking, PR integration, fleet health, cross-kild overlap detection, config hierarchy, daemon PTY ownership, daemon IPC, daemon-mode session create/open/stop/destroy, terminal attach with scrollback replay, PTY exit notification with state transitions, background daemonization, tmux shim with 16 commands, agent teams in daemon sessions, session resume (`--resume`), desktop notifications (`--notify`), lazy daemon status sync, daemon auto-start on create, task list persistence across sessions (`CLAUDE_CODE_TASK_LIST_ID`).

**What's been delivered in Phase 2 (2.1–2.9):** Click kild → terminal in worktree, terminal persistence across switching, multiple terminals per kild (tab bar), daemon-backed terminals with on-the-fly session creation. Terminal resize and scrollback UI, project rail, sidebar restructure, dashboard fleet cards, detail drill-down, terminal tabs with keyboard navigation, 2x2 pane grid, status bar with contextual alerts and keyboard hints.

**What's next:** Phase 2 complete. Phase 3 (Agent Intelligence) is next. See [phase-2-multiplexer-ux.prd.md](../prds/phase-2-multiplexer-ux.prd.md) for Phase 2 retrospective.

---

## The Phases

### Phase 0: GPUI Foundation --- DONE (PR #293)

**Goal:** Adopt gpui-component library, replacing hand-built UI components.

**What shipped:**
- [x] Added `gpui-component 0.5.1` from crates.io (works with existing `gpui 0.2.2`)
- [x] Migrated Button → `gpui_component::button::Button` (~26 call sites, 7 files)
- [x] Migrated TextInput → `gpui_component::input::{Input, InputState}` (stateful, native keyboard)
- [x] Migrated Modal → inlined dialog rendering with `cx.theme()` integration
- [x] StatusIndicator kept as custom component (no equivalent for glow effects)
- [x] Tallinn Night theme bridge maps brand colors to gpui-component tokens
- [x] 447 lines of hand-built component code deleted, +1,197 / -1,067 across 20 files
- [x] Full build passes: fmt, clippy, tests, build

**Note:** GPUI source stays on crates.io 0.2.2 for now. Switch to Zed git repo deferred to Phase 1a when terminal rendering APIs are needed.

**Research artifacts:** `.claude/PRPs/plans/phase-0-research/` (3 research docs from agent team)

---

### Phase 1a: Terminal Rendering Spike --- DONE (PR #300)

**Goal:** Render a live terminal session inside kild-ui using GPUI. Prove the hardest technical piece.

**Why:** This is the core technical risk. If we can't render terminals performantly in GPUI, the entire daemon vision needs a different UI strategy. Everything else is software engineering — this is the unknown.

**What shipped:**
- [x] Added `alacritty_terminal` dependency (VT100 parsing + terminal grid state)
- [x] Added `portable-pty` for PTY management (already a dependency via kild-daemon; proven in Phase 1b)
- [x] Built `TerminalElement` — custom GPUI Element (request_layout → prepaint → paint) that reads `alacritty_terminal::Term` grid and paints via scene primitives (quads for backgrounds, shaped text runs for glyphs)
- [x] ANSI color mapping to Tallinn Night theme — 16 named colors (standard + bright + dim), 256-color indexed (6×6×6 cube + 24-step grayscale), truecolor passthrough
- [x] Keyboard input → PTY stdin via escape sequence translation with app cursor mode support (vim/tmux)
- [x] Ctrl+T toggles between 3-column kild layout and full-area terminal view
- [x] 4ms event batching (250Hz max refresh, 100-event cap) to prevent UI freeze on large output
- [x] FairMutex between PTY reader thread and GPUI renderer to prevent lock starvation
- [x] Structured `TerminalError` with `error_code()` / `is_user_error()` methods, 7 typed variants
- [x] Graceful error handling — PTY creation failure shows UI error banner instead of crashing
- [x] All silent failure sites addressed with structured tracing (zero `let _ =` on fallible ops)
- [x] Encapsulated `BatchedTextRun` type with private fields and validated constructor
- [x] 27 new unit tests for color mapping and input translation
- [x] 1,410 lines of new terminal module code, +1,639 / -17 across 16 files
- [x] **Repaint signaling fixed** — batching task moved to `cx.spawn()` with `this.update(cx, |_, cx| cx.notify())` after each batch. No more input lag.

**New files in `crates/kild-ui/src/terminal/`:**
- `state.rs` — Terminal struct wrapping `Term<T>` + FairMutex + PTY lifecycle
- `terminal_element.rs` — Custom 3-phase Element with text run batching and background region merging
- `terminal_view.rs` — GPUI View with focus management and keyboard routing
- `colors.rs` — ANSI → Tallinn Night Hsla color mapping (Named, Indexed, Spec)
- `input.rs` — Keystroke → escape sequence translation
- `errors.rs` — Structured TerminalError with 7 variants
- `types.rs` — Encapsulated BatchedTextRun for rendering pipeline
- `mod.rs` — Module exports

**Decisions made:**
- **VT100 library:** `alacritty_terminal` — battle-tested, used by Alacritty and Zed, provides grid state + event system
- **Threading model:** Blocking PTY reader on dedicated OS thread (not GPUI BackgroundExecutor) because blocking reads would starve the cooperative async pool
- **Lock strategy:** `FairMutex` from alacritty_terminal prevents renderer from starving PTY reader (or vice versa)
- **Batching:** 4ms window (250Hz) with 100-event cap — balances responsiveness vs. render overhead
- **GPUI version:** Stayed on crates.io 0.2.2 — terminal rendering works with published API, no need for git-sourced GPUI

**Remaining gaps (Phase 2 prerequisites) — ALL RESOLVED:**
- [x] ~~**Terminal resize**~~ — `ResizeHandle` tracks dimensions, `resize_if_changed()` sends SIGWINCH on prepaint. Dynamic cols/rows. Resolved in Phase 2.1.
- [x] ~~**Scrollback UI**~~ — Mouse scroll via `alacritty_terminal::grid::Scroll`, display offset tracking, "Scrollback" badge when scrolled up. Resolved in Phase 2.1.
- [x] ~~Selection and copy/paste~~ — Cmd+C copies selection, Cmd+V pastes from clipboard. Resolved in Phase 2.1.
- [ ] Cursor blink animation — cursor renders as static block, no blink timer
- [ ] Wide character rendering — spacer cells skipped correctly but no explicit width-2 glyph handling (partial)
- [x] ~~URL detection / clickable links~~ — Cmd+Click opens URLs in browser. Resolved in Phase 2.1.

**Key reference:** Zed's terminal implementation uses the same pattern — `TerminalElement` with text run batching and background region merging for performance. Studied but not copied; written for KILD's needs.

**Signals of completion:** A shell session renders in kild-ui. You can type commands, see output, ANSI colors work, cursor tracks position. Ctrl+T toggles between dashboard and terminal.

**Scope:** ~1,500 lines of new code as estimated. The rendering pipeline was the hard part — proved feasible.

---

### Phase 1b: KILD Daemon --- DONE (PR #294 + follow-ups)

**Goal:** KILD daemon owns PTYs and persists sessions. Agents survive terminal disconnects.

**Why:** This is the core architectural shift. Today, closing a terminal kills the agent. With the daemon, agents run in daemon-owned PTYs — close your laptop, reopen, reconnect. The daemon is the foundation that Phase 2 (multiplexer UI), Phase 3 (intelligence), and everything after builds on.

**What shipped:**
- [x] New crate: `kild-daemon` (~4k lines) — tokio-based async daemon
  - `kild daemon start` — starts daemon (foreground or background), writes PID file, listens on `~/.kild/daemon.sock`
  - `kild daemon stop` — graceful shutdown via IPC
  - `kild daemon status` — show daemon running state
  - JSONL IPC server over unix socket with per-connection handlers
  - PTY lifecycle management via `portable-pty` (create, resize, destroy)
  - Session state machine (Creating → Running → Stopped) with multi-client attachment
  - Signal handling (SIGTERM/SIGINT) and graceful shutdown
  - PTY output broadcasting with scrollback ring buffer
  - PID file management
  - 15 integration tests covering full client-server protocol and lifecycle
- [x] `kild create` gains `--daemon` / `--no-daemon` flags with `resolve_runtime_mode()` helper
- [x] `[daemon] enabled` and `[daemon] auto_start` config fields (`DaemonRuntimeConfig`)
- [x] `RuntimeMode` enum in kild-core state types, `daemon_session_id` on `AgentProcess`
- [x] `kild attach <branch>` — raw terminal mode, stdin forwarding, PTY output streaming, Ctrl+B d detach
- [x] `kild stop` / `kild destroy` work with daemon sessions (IPC to daemon instead of PID kill)
- [x] Sync daemon client in kild-core (`daemon::client`) for CLI consumers
- [x] Typed `DaemonClient` in kild-daemon with connection pooling and all operations
- [x] CLI without daemon continues to work exactly as today (Ghostty, iTerm, etc.)
- [x] +6,029 / -115 across 47 files

**Follow-up work shipped (PRs #299, #303):**
- [x] Ping message handled by server — returns `Ack` (PR #299)
- [x] Scrollback replay on attach — ring buffer contents sent as `PtyOutput` before live stream begins (PR #299)
- [x] PTY exit notification — `PtyExitEvent` propagated to session manager, transitions session to Stopped (PR #299)
- [x] Background daemonization — `kild daemon start` spawns detached process, `--foreground` for debug (PR #299)
- [x] 15 integration tests covering ping, create, attach, output, multi-client, exit transitions, error cases (PR #299)
- [x] Lazy daemon status sync — `kild list`/`kild status` auto-updates stale daemon sessions (PR #299)
- [x] `kild open` gains `--daemon`/`--no-daemon` flags for daemon runtime mode (PR #299)
- [x] Login shell fix — `use_login_shell` flag in IPC, agent commands wrapped in `$SHELL -lc 'exec <command>'` (PR #303)

**Decisions made:**
- **PTY library:** `portable-pty` (cross-platform, used by Wezterm, flexible for daemon ownership)
- **Daemon lifecycle:** `kild daemon start` / `kild daemon stop` / `kild daemon status` subcommands
- **Default mode:** `[daemon] enabled` in config.toml + `--daemon` / `--no-daemon` flags override per-command
- **Socket path:** `~/.kild/daemon.sock` (user-global, one daemon per user)
- **IPC protocol:** Unix socket + JSONL with message ID correlation
- **Attach modes:** Terminal (`kild attach`) with Ctrl+B d detach. GUI attach deferred to Phase 2.

**Remaining gaps:**
- [x] ~~**Auto-start daemon** on `kild create --daemon` when daemon not running~~ — shipped in PR #323, moved from CLI to kild-core `daemon::autostart` module
- [ ] `kild ui <branch>` — open kild-ui focused on a daemon session (deferred to Phase 2)

**Research artifacts:** `.claude/PRPs/plans/phase-1b-integration-plan.md`

---

### Phase 1c: tmux Shim --- DONE (PRs #301, #306)

**Goal:** Make Claude Code agent teams work transparently inside KILD sessions via a tmux-compatible shim.

**Why:** Claude Code agent teams (experimental, Feb 2026) use tmux for split-pane mode — they check `$TMUX` and issue tmux CLI commands to create panes for teammates. A shim that intercepts these commands and routes them to the KILD daemon would make agent teams work inside KILD without any Anthropic cooperation.

**What shipped:**
- [x] New crate: `kild-tmux-shim` (3,055 lines) — standalone binary, no kild-core dependency
- [x] Hand-rolled tmux argument parser (not clap) — 16 commands + aliases, 62 parser tests
- [x] **Commands implemented:**
  - `split-window` — creates daemon PTYs for teammate panes via IPC
  - `send-keys` — writes to PTY stdin with key name translation (Enter, Tab, C-x, etc.)
  - `capture-pane` — reads PTY scrollback for agent team monitoring (PR #306)
  - `list-panes` — lists panes with format expansion (`#{pane_id}`, etc.)
  - `kill-pane` — destroys panes via daemon IPC with graceful error handling
  - `display-message` — expands tmux format strings
  - `select-pane` — sets pane style/title
  - `set-option` — stores pane/window/session options
  - `has-session` — checks session existence
  - `new-session` — creates sessions with initial pane
  - `new-window` — adds windows to sessions
  - `list-windows` — lists windows with filtering
  - `break-pane` / `join-pane` — hide/unhide panes
  - `select-layout` / `resize-pane` — no-ops (layout is meaningless without real tmux)
  - `version` — reports `tmux 3.4`
- [x] File-based pane registry at `~/.kild/shim/<session_id>/panes.json` with flock concurrency control
- [x] Sync JSONL IPC client over Unix socket (independent of kild-core)
- [x] 90 unit tests across parser, state, commands, and IPC
- [x] kild-core integration:
  - `ensure_shim_binary()` symlinks `~/.kild/bin/tmux` → `kild-tmux-shim` (one-time setup)
  - `build_daemon_create_request()` injects `$TMUX`, `$TMUX_PANE`, `$KILD_SHIM_SESSION`, PATH into daemon PTY env
  - ZDOTDIR wrapper preserves PATH after macOS `path_helper` reordering
  - `destroy_session()` cleans up child shim PTYs and `~/.kild/shim/<session>/` directory
- [x] File-based logging via `KILD_SHIM_LOG` env var
- [x] **Real-world tested** with Claude Code agent teams in daemon sessions

**Decisions made:**
- **Parser:** Hand-rolled, not clap. tmux arg syntax is non-standard (flags mixed with positional args) — clap would fight it.
- **State:** File-based, not in-memory. Shim is invoked per-command (no long-running process), so state must persist across invocations. flock for concurrency.
- **IPC:** Direct sync Unix socket client. No kild-core dependency — keeps shim binary small and fast.
- **Scope:** 16 commands, not the "~500 lines" originally estimated. Real-world testing revealed the full surface area needed.

**Known limitations:**
- Format expansion is basic (`#{pane_id}`, `#{session_name}`, etc.) — not full tmux format spec
- `select-layout` and `resize-pane` are no-ops (could add `resize_pty` IPC later)

**Signals of completion:** ✓ You can `kild create foo --daemon`, run Claude Code inside it, tell it to create an agent team, and the teammates spawn as daemon-managed PTYs. The lead can read teammate output via `capture-pane`.

---

### Phase 1 Bonus: Session Resume, Notifications, Task Lists, Bug Fixes (PRs #305, #308, #319, #317, #318, #322, #323)

Features and fixes shipped that weren't in the original Phase 1 plan but strengthen the daemon story:

**Session Resume (PR #308, closes #295):**
- [x] `kild open --resume` restores Claude Code conversation context after `kild stop`
- [x] Deterministic UUID generated on `kild create`, injected as `--session-id <uuid>` into agent command
- [x] `agent_session_id` stored at Session level (survives `clear_agents()` on stop)
- [x] On resume, `--resume <uuid>` injected to restore full conversation history
- [x] Non-Claude agents get clear error on `--resume` (fail fast, no silent fallback)
- [x] 18 resume-specific tests

**Desktop Notifications (PR #305, closes #246):**
- [x] `kild agent-status --notify` flag for desktop notifications
- [x] Platform-native dispatch: macOS `osascript` (Notification Center), Linux `notify-send`
- [x] Conditional: only fires on `Waiting` or `Error` status
- [x] New `notify` module in kild-core

**Task List Persistence (PR #319, closes #296):**
- [x] `CLAUDE_CODE_TASK_LIST_ID` env var injected for Claude agents on create and open
- [x] Deterministic task list ID: `kild-{project_id}-{branch}`
- [x] On resume: reuses existing ID so tasks survive across restarts
- [x] On fresh open: generates new ID for clean task list
- [x] `kild destroy` cleans up `~/.claude/tasks/{task_list_id}/` directory
- [x] Terminal mode uses `env` command for exec-compatible env var injection
- [x] Daemon mode passes env vars via IPC
- [x] Agent-specific via `AgentBackend` trait — only Claude agents get the env var

**Daemon Auto-Start (PR #323, closes #312):**
- [x] `kild create --daemon` auto-starts daemon if not running (when `auto_start = true` in config)
- [x] Logic moved from CLI into kild-core `daemon::autostart` module
- [x] Works for both `kild create` and `kild open` in daemon mode

**Bug Fixes:**
- [x] Daemon re-open after stop (PR #317, closes #309) — `kild stop` now uses `destroy_daemon_session` so re-open works
- [x] Flaky daemon ping test (PR #318, closes #307) — isolated tests from running daemon
- [x] `kild open --no-agent` status (PR #322, closes #257) — bare shell open now sets status to Active

---

### Phase 2: Multiplexer UX — IN PROGRESS (Phases 2.1–2.4 complete)

**Goal:** kild-ui becomes the full multiplexer from the mockup — project rail, navigation-only sidebar, three swappable views (Control pane grid, Dashboard fleet cards, Kild Detail drill-down), keyboard nav. The user's primary interface for managing parallel AI agent sessions.

**Why:** This is what users see. The daemon (Phase 1b) gives us persistence and PTY ownership. This phase gives us the UX that makes it feel like a product, not a tech demo.

**Prerequisites from Phase 1a — ALL RESOLVED:**
- [x] ~~**Terminal resize**~~ — `ResizeHandle` + `resize_if_changed()` in terminal_element.rs
- [x] ~~**Scrollback UI**~~ — Mouse scroll, display offset, "Scrollback" badge

**Delivered (Phases 2.1–2.4):**

- [x] **Spike 1: smol IO on GPUI executor** — `smol::Async<UnixStream>` works on `BackgroundExecutor`, <2ms roundtrip (PR #404)
- [x] **Spike 2: Daemon-backed terminal rendering** — Daemon PtyOutput renders identically to local PTY through `alacritty_terminal::Term` → `TerminalElement` (PR #407)
- [x] **Phase 2.1: Terminal in worktree** — Click kild → local PTY terminal in worktree directory, Ctrl+Escape back to dashboard (PR #411)
- [x] **Phase 2.2: Terminal persistence** — `HashMap<session_id, TerminalTabs>` keeps terminals alive across kild switching
- [x] **Phase 2.3: Multiple terminals per kild** — Tab bar with +/× for multiple terminals, Ctrl+Tab cycling, tab rename
- [x] **Phase 2.4: Daemon-backed terminals** — "+" menu offers Local vs Daemon terminals, on-the-fly daemon session creation, daemon auto-start from UI
- [x] **Session lifecycle in UI** — Create, Open, Stop, Destroy dialogs all work from within kild-ui. No external terminal needed for session management.

**Completed (Phases 2.5–2.7):**

- [x] **Phase 2.5: Extract + keyboard nav** — Refactor main_view.rs, ⌘J/K kild navigation, ⌘1–9 jump (PR #414)
- [x] **Phase 2.6: Project rail + sidebar + view shell** — 48px rail, navigation-only sidebar with hover actions, top tab bar (Control/Dashboard), ⌘D toggle, monochrome button discipline (PR #416)
- [x] **Phase 2.7: Dashboard + detail views** — Fleet summary + kild cards, click card → detail drill-down (session, git, PR, terminals, actions) (PR #417)

**Remaining (Phases 2.8–2.9) — see [phase-2-multiplexer-ux.prd.md](../prds/phase-2-multiplexer-ux.prd.md) for detailed plan:**

- [x] **Phase 2.8: Control view — pane grid** — 2x2 grid showing terminals from multiple kilds, focus routing, maximize/close (PR #418)
- [x] **Phase 2.9: Status bar + polish** — Footer with contextual alerts (dirty worktrees, errors) and view-aware keyboard hints (Kbd widget). Dead code cleanup.

**Key reference:** [mockup-v2.html](../branding/mockup-v2.html) is the definitive mockup. Three-view architecture (Control/Dashboard/Detail), navigation-only sidebar, monochrome buttons.

**Signals of completion:** The mockup is real. You can switch projects, browse kilds, see terminals in a 2x2 pane grid, scan fleet status on the dashboard, drill into kild details. kild-ui replaces external terminal windows as the primary interface.

**Scope:** High. Incremental delivery via subphases. Leverage gpui-component (Resizable, Sidebar, Kbd, Tabs) heavily.

---

### Phase 3: Agent Intelligence

**Goal:** KILD understands what agents are doing and acts on that knowledge. Checkpointing, notifications, cross-session awareness.

**Why:** This is where KILD becomes more than a terminal multiplexer. tmux shows you terminal output. KILD shows you agent state — thinking, editing, stuck, idle, waiting for permission. This is the differentiator.

**Deliverables:**
- [ ] Agent state tracking — hook integration + file watching for real-time agent state (thinking/editing/stuck/idle/done)
- [ ] Notification system — contextual alerts when agents need attention, finish tasks, hit errors (foundation: `--notify` flag shipped in Phase 1)
- [ ] Checkpoint engine — periodic snapshots of session state (git SHA + scrollback + agent context)
- [ ] Restore from checkpoint — recover after daemon crash or manual rollback
- [ ] Cross-session conflict detection — continuous `collect_file_overlaps()` (foundation exists in `kild overlaps`)
- [ ] Agent-aware rendering — status indicators, thinking animation, tool call cards inline with terminal output (stretch)

**Signals of completion:** You see "agent thinking..." in the UI. You get notified when an agent finishes. You can checkpoint and restore a session.

**Scope:** Medium-High. Builds on Phase 1b daemon infrastructure.

---

### Phase 4: Session Orchestration

**Goal:** Session forking, session graphs (DAG), templates, fleet command center.

**Why:** This is the "six innovations" layer from the research doc. No other tool has session forking or dependency graphs between sessions. This is where KILD becomes categorically different.

**Deliverables:**
- [ ] Session forking — try parallel approaches from same checkpoint
- [ ] Session graph (DAG) — dependencies between sessions, auto-start downstream when upstream completes
- [ ] Templates — reusable workflow definitions in TOML
- [ ] Fleet dashboard — merge readiness overview, session graph visualization, bulk operations
- [ ] Tool call rendering — syntax-highlighted cards inline with terminal (if not done in Phase 3)

**Signals of completion:** You can fork a session, try two approaches, pick the winner. You can define "when auth finishes, start api-integration automatically."

**Scope:** High. Significant new concepts and UI.

---

### Phase 5: Portability & Remote

**Goal:** Sessions move between machines. Remote daemon access. Mobile monitoring.

**Why:** The Tōryō-from-anywhere vision. Start locally, continue on cloud. Monitor from your phone.

**Deliverables:**
- [ ] Session serialization — export/import between machines
- [ ] IPC over TCP — remote daemon access (upgrade unix socket to TCP/TLS)
- [ ] Remote kild-ui — connect to daemon on another machine
- [ ] Mobile client (read-only) — status monitoring, approve/deny decisions
- [ ] Cloud deployment — daemon on VPS, agents in containers

**Signals of completion:** You deploy a daemon on a VPS. Connect kild-ui from your laptop. Agents run in the cloud. You check in from your phone.

**Scope:** Very high. Network protocol, auth, mobile client.

---

## Dependencies Between Phases

```
Phase 0 (DONE) ──→ Phase 1a (DONE) ──→ Phase 2 (2.1-2.4 DONE, 2.5-2.9 IN PROGRESS)
                                           ↑
Phase 1b (DONE) ──→ Phase 1c (DONE)  ─────┘
       ↓
  Phase 3 ──→ Phase 4 ──→ Phase 5
```

- **Phase 0** (DONE) — gpui-component adopted
- **Phase 1a** (DONE) — terminal rendering in GPUI via alacritty_terminal + portable-pty
- **Phase 1b** (DONE) — daemon crate with PTY ownership, IPC, session state machine, scrollback replay, exit notification, background mode
- **Phase 1c** (DONE) — tmux shim with 16 commands, agent teams work in daemon sessions
- **Phase 1 bonus** — session resume (`--resume`), desktop notifications (`--notify`), task list persistence, daemon auto-start, bug fixes (#309, #307, #257)
- **Phase 2** (multiplexer UX) — DONE. All subphases (2.1–2.9) complete.
- **Phase 3** (intelligence) needs Phase 1b (DONE) daemon for hooks and state tracking
- **Phase 4** and **Phase 5** are sequential and build on everything before them

## Open Bugs (Fix Before Phase 2)

| Issue | Title | Priority | Status |
|-------|-------|----------|--------|
| ~~#309~~ | ~~`kild open --daemon` fails after `kild stop`~~ | ~~P0~~ | Fixed (PR #317) |
| ~~#307~~ | ~~Flaky test: `test_ping_daemon_returns_false_when_not_running`~~ | ~~P1~~ | Fixed (PR #318) |
| ~~#257~~ | ~~`kild open --no-agent` doesn't set status to active~~ | ~~P1~~ | Fixed (PR #322) |
| #289 | Ghostty focus/hide issues after Core Graphics migration | P1 | Open |

## What Stays the Same

The CLI and external terminal workflow is **not going away**. The daemon is a parallel runtime option. Power users who prefer `kild create foo` → Ghostty window keep that workflow forever.

Stable through all phases:
- Terminal backends (`terminal/` module) — Ghostty, iTerm, Terminal.app, Alacritty
- Session handlers (`sessions/`) — create, open, stop, destroy, complete
- Git worktree operations (`git/` module)
- Config hierarchy (`config/` module)
- Forge/PR operations (`forge/` module)
- Health monitoring (`health/` module)
- Cleanup strategies (`cleanup/` module)
- File inclusion patterns (`files/` module)
- Agent backend definitions (`agents/` module)
- CLI stays fully functional — daemon is additive

## What Gets Added (Not Replaced)

The daemon is **additive**. External terminal backends (Ghostty, iTerm, Terminal.app, Alacritty) stay fully functional for CLI users. The daemon is an optional runtime — you can still `kild create foo` and get an agent in a Ghostty window.

- New `kild-daemon` crate — PTY ownership, IPC server, session persistence (Phase 1b, DONE)
- New `kild-tmux-shim` crate — tmux command translation for agent teams (Phase 1c, DONE)
- New terminal rendering module in kild-ui — GPUI Element for embedded terminals (Phase 1a, DONE)
- kild-ui views — rewrite for multiplexer layout (Phase 2)

## Key Technical Decisions

### Decided
- **Daemon lifecycle:** `kild daemon start` / `kild daemon stop` subcommands
- **Daemon scope:** One daemon per user, manages all projects
- **Default mode:** Config `[daemon] enabled = true` + `--daemon` / `--no-daemon` flags
- **Socket path:** `~/.kild/daemon.sock`
- **IPC protocol:** Unix socket + JSONL. gRPC deferred to Phase 5 (remote access).
- **Attach modes:** `kild attach foo` (terminal) + `kild ui foo` (GUI)
- **Additive architecture:** External terminal backends stay. Daemon is opt-in.
- **Multiplexer UI:** GPUI (native desktop), not ratatui TUI. Primary reasons: 7,500 lines of existing kild-ui code, GPU-accelerated terminal rendering, mixed content (terminals + dashboards + notifications), mouse/font support. A lightweight ratatui TUI client may be added later as a secondary interface (e.g., for SSH access), but is not on the roadmap.

### Resolved
- **PTY library:** `portable-pty` — decided and shipped in Phase 1b (PR #294). Cross-platform, used by Wezterm, flexible for daemon ownership.
- **GPUI version:** Staying on crates.io 0.2.2. Terminal rendering works with published API. Split pane resize confirmed available via `gpui_component::resizable` (0.5.1). No need to switch.
- **tmux shim scope:** 16 commands discovered through real-world testing with Claude Code agent teams. Hand-rolled parser (not clap). File-based pane state with flock. 3,055 lines — 6x original estimate.
- **Terminal resize:** Resolved in Phase 2.1. `ResizeHandle` + `resize_if_changed()` sends SIGWINCH on prepaint when element bounds change.
- **Scrollback UI:** Resolved in Phase 2.1. Mouse scroll via `alacritty_terminal::grid::Scroll`, display offset tracking, "Scrollback" badge.
- **Cmd key for shortcuts:** `event.keystroke.modifiers.platform` maps to ⌘ on macOS. Already used for Cmd+C/V in terminal_view.rs. All Phase 2.5+ keyboard shortcuts use ⌘.
- **Pane grid resize:** `gpui_component::resizable` (0.5.1) provides `h_resizable()` / `v_resizable()` with `ResizablePanelGroup` and built-in resize handles. 2x2 grid starts with equal sizing; resizable panels optional.
- **Daemon terminal semantics:** Closing a daemon terminal view does NOT stop the daemon process. The terminal is a window into a running process. Removing from pane grid = close view, daemon keeps running.

---

*This epic is a living document. Each phase gets its own detailed plan when we start it.*
*Last updated: 2026-02-13 — Phase 2 complete (all subphases 2.1–2.9). Phase 3 (Agent Intelligence) is next.*
