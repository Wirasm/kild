# Phase 2: Multiplexer UX

## Problem Statement

KILD manages 3-15 parallel AI agent sessions across multiple projects. Today, the Tōryō switches between Ghostty windows, runs `kild list`, and uses `kild attach` in separate terminals. The existing kild-ui shows a metadata dashboard with a single toggled terminal (Ctrl+T) that always opens in the home directory — it's not connected to any specific kild.

The core problem: **there's no way to click a kild and get a terminal in its worktree directory.** Everything else — daemon persistence, agent teams, split panes — is layered on top of that fundamental capability.

## Evidence

- The existing kild-ui terminal (Ctrl+T) opens in `$HOME`, not in the kild's worktree — it's useless for kild work
- Each kild has a `worktree_path` that the terminal should `cd` into, but this data is never used
- With 5+ kilds running, window switching becomes the bottleneck — not the agents themselves
- `kild list` shows status but can't show live terminal output
- The daemon and terminal rendering pipeline are proven (Spike 1 + Spike 2) but not yet connected to the kild selection flow

## Proposed Solution

Evolve kild-ui into a terminal multiplexer by starting from the simplest possible change — making `Terminal::new()` accept a working directory — and incrementally adding multiplexer features. Each phase produces a usable improvement.

The end state is the same as the mockup (`mockup-v2.html`): Rail | Sidebar | Main area with three swappable views (Control pane grid, Dashboard fleet cards, Kild Detail drill-down). But we get there bottom-up instead of building the full layout shell first.

## Key Hypothesis

If clicking a kild instantly gives me a working terminal in its worktree, I'll stop reaching for external terminal windows. Every subsequent feature (persistent terminals, daemon mode, splits) amplifies that core value.

## What We're NOT Building

- **ratatui TUI** — GPUI is the primary interface
- **Phase 3 intelligence** — No agent state parsing from terminal output
- **Remote access (yet)** — Local daemon only for now. Future: daemon on VPS/remote, accessible from anywhere including mobile. Not Phase 2.
- **Settings UI** — Config stays in TOML files
- **Breaking CLI behavior** — `kild create` with external terminals stays as-is
- **Completed/destroyed kild archiving** — Future feature. Completed/destroyed kilds disappear from navigation. Archive/stats tracking comes later.

## Success Metrics

| Metric | Target | How Measured |
|--------|--------|--------------|
| Daily driver | Use kild-ui instead of Ghostty windows for agent monitoring | Personal usage switch |
| Terminal fidelity | ANSI rendering matches external terminal quality | Visual comparison |
| Switching speed | < 100ms to switch between kild terminals | Perceived latency |
| Core flow works | Click kild → working terminal in worktree | Feature exists |

## Open Questions

- [ ] How to handle PTY streaming performance with 10+ simultaneous terminals? Only render visible panes?
- [ ] Should daemon mode be opt-in per kild or a global default? (Direction: daemon-by-default eventually, opt-in for now)
- [ ] Command palette (⌘K) for quick navigation — build custom or wait for gpui-component support? (Deferred past Phase 2)
- [x] ~~How to discover teammate panes from daemon sessions?~~ RESOLVED: Read shim pane registry files. File watching for live updates. (Phase 6 — experimental)
- [x] ~~Should `smol::Async<UnixStream>` run directly on GPUI's `BackgroundExecutor`?~~ RESOLVED: Spike 1 validated — works directly, <2ms roundtrip.
- [x] ~~Do daemon bytes render correctly in alacritty_terminal?~~ RESOLVED: Spike 2 validated — identical to local PTY.
- [x] ~~Does GPUI support Cmd key modifier in `on_key_down`?~~ RESOLVED: Yes. `event.keystroke.modifiers.platform` maps to ⌘ on macOS. Already used for Cmd+C/V in `terminal_view.rs`.
- [x] ~~Does GPUI 0.2.2 / gpui-component 0.5.1 have resizable split panes?~~ RESOLVED: Yes. `gpui_component::resizable` provides `h_resizable()` / `v_resizable()` with `ResizablePanelGroup`, `ResizablePanel`, `ResizableState`, and built-in resize handles. No custom mouse tracking needed.

---

## Pre-Implementation Spikes

### Spike 1: smol IO on GPUI Executor — `complete` ✅
- **Goal**: Prove `smol::Async<UnixStream>` works on GPUI's `BackgroundExecutor`
- **Result**: **SUCCESS**. Ping/Ack roundtrip completes in <2ms. async-io's global reactor correctly wakes futures on GPUI's GCD-based task scheduler.
- **Branch**: `kild/spike1-smol-gpui-executor`
- **Key finding**: `smol::Async<UnixStream>` works directly on `BackgroundExecutor` with no dedicated thread. Write to owned stream, hand ownership to `BufReader` for reading. No channel bridge needed.
- **Gotcha**: Rust method resolution prefers deref to `Async<T>` over the `&Async<T>` impl for `AsyncWrite`. Use owned stream for sequential write-then-read, not shared references.
- **Architecture confirmed**: Phase 4 async client uses `smol::Async<UnixStream>` directly on `BackgroundExecutor`. No dedicated thread, no channel bridge.
- **PR**: [#404](https://github.com/Wirasm/kild/pull/404)

### Spike 2: Daemon-Backed Terminal Rendering — `complete` ✅
- **Goal**: Prove daemon `PtyOutput` bytes render correctly through `alacritty_terminal::Term` → `TerminalElement`
- **Result**: **SUCCESS**. Daemon PtyOutput renders identically to local PTY. Shell prompts with ANSI colors, `ls` output, interactive commands, scrollback replay — all work correctly. Keystrokes route through WriteStdin IPC. No encoding or framing issues.
- **Branch**: `kild/spike2-daemon-terminal-rendering`
- **Key finding**: The `UnboundedReceiver<Vec<u8>>` channel feeding `run_batch_loop()` is the perfect interface boundary. Daemon bytes (base64-decoded PtyOutput) feed the same channel as local PTY bytes. The entire rendering pipeline (VTE parser, TerminalElement, colors) is completely unchanged.
- **Architecture**: Two-connection attach pattern — one `smol::Async<UnixStream>` for streaming PtyOutput reads, one for WriteStdin/ResizePty/Detach writes. `DaemonPtyWriter` bridges sync `Write` trait to async IPC via `futures::channel::mpsc::unbounded`. `ResizeTarget` enum abstracts local PTY vs daemon resize.
- **Gotcha**: Ctrl+D handler needs `cx.spawn_in(window, ...)` (not `cx.spawn(...)`) to get `AsyncWindowContext` with Window access for `TerminalView::from_terminal()`.
- **PR**: [#407](https://github.com/Wirasm/kild/pull/407)

---

## Users & Context

**Primary User**
- **Who**: The Tōryō — solo developer managing 3-15 parallel AI agent sessions across 1-3 projects
- **Current behavior**: Switches between Ghostty windows, runs `kild list` / `kild attach` / `kild health` in a dedicated terminal. Loses track of which agents are idle, working, or waiting.
- **Trigger**: Having more than 2 sessions running simultaneously — window switching becomes unmanageable
- **Success state**: One window shows all sessions. Click a kild, get a terminal. Open more terminals. Switch instantly.

**Job to Be Done**
When I have 5+ AI agents running in parallel, I want to click a kild and immediately get a terminal in its worktree, so I can monitor, interact, and direct agents without leaving kild-ui.

**Non-Users**
This is NOT for users who prefer CLI-only workflows (they keep using `kild attach` + external terminals). NOT for users who run a single agent at a time.

---

## Solution Detail

### Core Capabilities (MoSCoW)

| Priority | Capability | Rationale |
|----------|------------|-----------|
| Must | Click kild → terminal in its worktree (local PTY) | The fundamental multiplexer value |
| Must | Terminal-per-kild persistence (switching doesn't kill terminals) | Multiplexer table stakes |
| Must | Multiple terminals per kild (tab bar) | Users need shell + agent in same kild |
| Must | Keyboard shortcuts for navigation | Terminal users expect keyboard-driven UX |
| Should | Daemon-backed terminals (PTY survives UI close) | Persistence for long-running agents |
| Should | 2x2 pane grid (Control view) | Multi-kild terminal viewing across project |
| Should | Dashboard view with fleet cards | Fleet awareness and kild health at a glance |
| Should | Kild detail view (dashboard drill-down) | Metadata at a glance: git stats, PR, path, terminals |
| Could | Minimized session bars | Additional fleet awareness below pane grid |
| Could | Status bar with notifications | Surface alerts without interrupting focus |
| Could | Daemon mode toggle in UI | User chooses local vs daemon per terminal |
| Won't (now) | Agent team / teammate discovery | Experimental — comes after core is solid |
| Won't | In-app session creation | CLI handles this. UI observes. |
| Won't | Agent state parsing | Phase 3 concern |

### User Flow (Critical Path)

1. Launch `kild ui` — loads session list from disk
2. See sidebar with kilds grouped by status (existing behavior)
3. Click a kild → main area shows a terminal in that kild's worktree
4. Click another kild → terminal switches, previous kild's terminal stays alive
5. Want another terminal in same kild? Click "+" → new tab
6. Switch projects via rail → sidebar filters kilds
7. Close kild-ui → local PTY terminals die, daemon terminals persist

---

## Technical Approach

**Feasibility**: HIGH — All building blocks exist and are proven.

**Key Technical Facts**
- `Terminal::new(cx)` spawns a local PTY. Needs a `cwd` parameter (currently hardcoded to home).
- `Terminal::from_daemon(session_id, conn, cx)` connects to daemon IPC. Already works (Spike 2).
- Each `Terminal` has its own `alacritty_terminal::Term` + `FairMutex`. No shared state between terminals. Can have many.
- `Session.worktree_path` (PathBuf) is available on every kild session.
- `TerminalView::from_terminal(terminal, window, cx)` wraps a Terminal in a GPUI Entity. Already works.

**Architecture**
- Terminal multiplexing lives in the view layer (`MainView`), not in `AppState`. A `HashMap<session_id, Vec<Entity<TerminalView>>>` tracks open terminals per kild.
- Local PTY terminals spawn the user's shell in the kild's worktree directory. No daemon needed.
- Daemon terminals reuse existing `daemon_client.rs` + `Terminal::from_daemon()`. Opt-in per terminal.
- CLI `kild create` with external terminals is unchanged. Those kilds appear in sidebar with metadata. User can open an embedded terminal if they want.

**Technical Risks**

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| PTY streaming performance with 10+ terminals | Medium | Medium | Only render visible terminal. Background terminals receive bytes but don't trigger repaint. |
| ~~GPUI split pane resize needs APIs not in 0.2.2~~ | ~~Low~~ | ~~Medium~~ | **RESOLVED**: `gpui_component::resizable` (0.5.1) provides `ResizablePanelGroup` with built-in resize handles. No custom mouse tracking needed. 2x2 pane grid starts with equal sizing; resizable panels optional. |
| Memory usage with many terminals open | Low | Medium | Drop terminals for completed/destroyed kilds. Lazy-create on click. |

---

## Implementation Phases

Each phase produces a usable improvement. No phase requires the next one.

| # | Phase | Description | Depends |
|---|-------|-------------|---------|
| 1 | Terminal in worktree | Click kild → local PTY terminal in its worktree directory | — |
| 2 | Terminal persistence | Switching kilds keeps terminals alive (HashMap) | 1 |
| 3 | Multiple terminals | Tab bar per kild with +/× for multiple terminals | 2 |
| 4 | Daemon terminals | Opt-in daemon-backed terminals alongside local | 3 |
| 5 | Layout and navigation | Rail, sidebar, 2x2 pane grid, dashboard, detail view, keyboard nav | 3 |
| 6 | Agent teams | Teammate discovery from shim registry (experimental) | 4 |

```
Phase 1 (terminal in worktree)
  → Phase 2 (persistence)
    → Phase 3 (tabs)
      → Phase 4 (daemon) → Phase 6 (agent teams, experimental)
      → Phase 5 (layout polish)
```

### Phase 1: Terminal in worktree — `complete` ✅
- **Goal**: Click a kild in the sidebar → get a working terminal in its worktree directory
- **Result**: **DONE**. Click kild → terminal in worktree. `pwd` confirms correct directory. Ctrl+Escape returns to dashboard.
- **Scope**:
  - Add `cwd: Option<PathBuf>` parameter to `Terminal::new()` via extracted `resolve_working_dir()`
  - On kild click in sidebar/list, create a terminal with `cwd = session.worktree_path`
  - Replace the existing 3-column layout with terminal when a kild is selected
  - Ctrl+T global terminal still works (uses home dir as before)
  - If worktree doesn't exist, returns `TerminalError::InvalidCwd` (surfaced in error banner)
  - Ctrl+Escape exits terminal (not plain Escape — that breaks vim/fzf/less)
- **Key changes**: `terminal/state.rs` (add cwd, resolve_working_dir), `views/main_view.rs` (wire sidebar click → terminal), `views/kild_list.rs` (pass window to click handler)
- **PR**: [#411](https://github.com/Wirasm/kild/pull/411)

### Phase 2: Terminal persistence
- **Goal**: Switching between kilds doesn't destroy terminals — they stay alive in background
- **Scope**:
  - `HashMap<String, Entity<TerminalView>>` in MainView keyed by session_id
  - On kild click: if terminal exists → show it; else → create and store
  - On kild destroyed/stopped: drop terminal entity (PTY cleanup happens via Drop)
  - Remove old Ctrl+T / Ctrl+D global terminal toggle (replaced by per-kild terminals)
- **Success signal**: Click kild A → type something → click kild B → click kild A → previous output still there
- **Key changes**: `views/main_view.rs` (add HashMap, selection-driven rendering)

### Phase 3: Multiple terminals per kild
- **Goal**: Each kild can have N terminals, switched via tab bar
- **Scope**:
  - Evolve HashMap value to `Vec<Entity<TerminalView>>` with active index
  - Tab bar above terminal area: one tab per terminal, "+" to add, "×" to close
  - Ctrl+Tab / Ctrl+Shift+Tab to cycle tabs
  - Each new terminal opens in same `worktree_path`
- **Success signal**: Open 3 terminals for one kild, switch between them via tabs
- **Key changes**: `views/main_view.rs` (tab bar rendering, tab state)

### Phase 4: Daemon-backed terminals
- **Goal**: User can open daemon-backed terminals that survive UI close
- **Scope**:
  - "+" button offers choice: local terminal or daemon terminal
  - Daemon terminal uses existing `daemon_client.rs` + `Terminal::from_daemon()`
  - Visual indicator on tab showing local vs daemon mode
  - On UI restart: can re-attach to daemon terminals (session discovery)
  - If daemon not running: option is grayed out with tooltip
- **Success signal**: Open daemon terminal → close kild-ui → reopen → terminal output preserved
- **Key changes**: `views/main_view.rs` (terminal creation flow), `daemon_client.rs` (session-specific attach)

### Phase 5: Layout and navigation
- **Goal**: Full multiplexer UX — rail, sidebar, three-view architecture, keyboard nav, fleet awareness
- **Scope**:
  - Project rail + sidebar restructure (navigation-only, terminals nested under kilds)
  - Three swappable main views: Control (2x2 pane grid), Dashboard (fleet cards), Kild Detail (drill-down)
  - Top tab bar for view switching (Control / Dashboard)
  - 2x2 pane grid showing terminals from multiple kilds within the project
  - Dashboard with fleet summary bar + scannable kild cards (click → Detail)
  - Kild Detail drill-down (session info, git, PR, terminals, actions) with back navigation
  - Cmd+J/K to navigate kilds in sidebar
  - Cmd+1-9 to jump to kild by index
  - Cmd+D to toggle Control/Dashboard
  - Status bar with keyboard shortcut hints
- **Success signal**: Full keyboard-driven multiplexer workflow with fleet awareness
- **Key changes**: New view components (project_rail, dashboard_view, detail_view, pane_grid, status_bar)

### Phase 6: Agent teams (experimental)
- **Goal**: Visualize and switch between agent team teammates within a kild
- **Scope**:
  - Read `~/.kild/shim/{session_id}/panes.json` to discover teammate panes
  - File-watch pane registry for live updates
  - Auto-create daemon terminal tabs for discovered teammates
  - Agent team tree in sidebar
- **Success signal**: Create agent team kild → teammate tabs appear automatically
- **Key changes**: New teammate discovery module, extend tab bar

---

## Decisions Log

| Decision | Choice | Alternatives | Rationale |
|----------|--------|--------------|-----------|
| Start with local PTY | Local first, daemon later | Daemon-only from start | Local PTY is simpler (no daemon dependency), proves the core UX. Daemon adds persistence on top. |
| Terminal state in view layer | HashMap in MainView | New state module | YAGNI — keep it in the view until we need more. No premature abstraction. |
| Multiple terminals = tabs not splits first | Tabs first, splits Phase 5 | Tabs + splits together | Tabs are simpler, splits add complexity (resize handles, focus routing). Sequential is safer. |
| Daemon as opt-in | User chooses per terminal | Always daemon if available | Some users may not run daemon. Local PTY is zero-config. Let users opt into persistence. |
| Agent teams last | Phase 6, experimental | Core Phase 4 | Agent teams depend on tmux shim, which is experimental. Core multiplexer works without it. |
| CLI unchanged | External terminals stay | Migrate all to daemon | Don't break existing workflows. UI observes CLI-created kilds. |
| No teammate discovery in core | Phase 6 only | Baked into every phase | Over-engineering trap. Core multiplexer = terminals in worktrees, not agent team management. |
| GPUI version | Stay on crates.io 0.2.2 | Switch to Zed git repo | Terminal rendering works. Split pane resize confirmed available in gpui-component 0.5.1 (`resizable` module). No need to switch. |
| Async runtime for kild-ui | smol (not tokio) | tokio, async-compat | GPUI is built on smol ecosystem. Validated in Spike 1. |
| kild-ui → kild-daemon dependency | No (kild-protocol only) | Direct dependency | Separate concerns. Three consumers, one protocol, three transports. |
| Cmd key for shortcuts | `modifiers.platform` (⌘ on macOS) | `modifiers.control` for everything | Platform-native feel. Ctrl reserved for terminal passthrough. Already proven with Cmd+C/V. |
| Split pane implementation | `gpui_component::resizable` | Custom mouse tracking | Library provides `ResizablePanelGroup` + resize handles out of the box. Don't hand-roll. |
| Navigation scope | All non-completed/non-destroyed kilds | Only running kilds | Stopped kilds are still active work. Completed/destroyed are archived (hidden from nav). |
| Command palette (⌘K) | Deferred past Phase 2 | Build in Phase 2.7 | No built-in palette in gpui-component. Custom build is significant. Core nav (⌘J/K, ⌘1-9) covers Phase 2 needs. |
| Detail view location | Dashboard drill-down (main area) | Sidebar inspect mode | Sidebar stays navigation-only. Detail as a main-area view keeps sidebar simple and scales better. Click dashboard card → Detail view. Escape/back → Dashboard. |
| Button color discipline | Monochrome only | Colored action buttons | Colors reserved for status indicators (aurora/copper/ember dots), agent names (kiri), and git stats (+/-). Buttons use frosted ice primary, border secondary, ghost, and danger-reveals-on-hover. Avoids "Christmas tree" UI. |
| Three-view architecture | Control / Dashboard / Detail in main area | Single terminal view | Top tab bar switches between views. Control = 2x2 pane grid. Dashboard = fleet cards. Detail = drill-down from dashboard card. All views swap in the same main area. |
| Daemon terminal semantics | UI view into running process | Terminal = process | Closing a daemon terminal view does NOT stop the process. Removing from pane grid = close view, daemon keeps running. Terminal is a window, not the process. |

---

## Conceptual Hierarchy

The data model follows a strict hierarchy that informs all UI navigation and layout decisions:

```
Project (e.g., "kild", "shards")
  └── Kild / Isolation (e.g., "feature-auth") — a worktree + environment
        └── Terminal (with or without daemon backing)
              └── Agent (lives inside terminal, not a first-class UI entity yet)
```

**Key semantics:**
- **Project**: A git repository. Rail icon filters sidebar to show only its kilds.
- **Kild (Isolation)**: A worktree-based environment. Lifecycle: created → running → stopped → completed/destroyed. Only created/running/stopped are navigable. Completed/destroyed are archived (future).
- **Terminal**: A PTY session, either local or daemon-backed. Multiple terminals per kild. Daemon terminals are *views into running processes* — closing the pane view doesn't stop the process. Terminals from different kilds can be displayed simultaneously in the 2x2 pane grid (Control view).
- **Agent**: Runs inside a terminal (e.g., Claude Code, Codex). Not managed by kild-ui directly. Agent teams (Phase 6) will add teammate discovery, but agents are not a navigation primitive — terminals are.

**Navigation rules:**
- ⌘J/K and ⌘1–9 navigate kilds (not terminals, not projects)
- Ctrl+Tab cycles terminals within a kild
- Rail switches projects
- Sidebar shows kilds grouped by status under the selected project, with terminals listed under each kild. Sidebar is navigation-only with hover actions (editor, stop/open) — no detail view in sidebar.
- Main area has three swappable views via top tab bar:
  - **Control**: 2x2 pane grid showing terminals from multiple kilds within the project
  - **Dashboard**: Fleet overview with kild cards (git stats, PR, terminals). Click card → Detail.
  - **Detail**: Kild drill-down from dashboard (session info, git, PR, terminals, actions). Escape/back → Dashboard.
- ⌘D toggles between Control and Dashboard
- Escape from Detail → Dashboard

**Future direction:**
- Daemon-by-default (all terminals backed by daemon, local PTY as fallback)
- Remote daemons on VPS — access from any device including mobile
- Completed kild archive with stats tracking
- Customizable pane grid layouts (beyond 2x2, user-defined pane counts and arrangements)

---

## Research Summary

**Technical Context**
- kild-ui: ~9k lines, GPUI 0.2.2, gpui-component 0.5.1, 3-column dashboard layout
- Terminal module: `alacritty_terminal` for VTE parsing, `portable-pty` for local PTY, custom GPUI Element
- Daemon IPC: JSONL over unix socket, Attach sends scrollback + broadcast stream, async tokio server
- `Terminal::new()` works for local PTY, `Terminal::from_daemon()` works for daemon IPC (both proven)
- `Session.worktree_path` available on every kild — the missing connection to terminal creation
- Each Terminal has its own `Term` + `FairMutex` — safe to run many in parallel

**Existing Code to Leverage**
- `terminal/state.rs` — `Terminal::new()` and `Terminal::from_daemon()` (add cwd param)
- `terminal/terminal_view.rs` — `TerminalView::from_terminal()` wraps Terminal in GPUI Entity
- `terminal/terminal_element.rs` — GPUI Element for terminal rendering (no changes needed)
- `terminal/colors.rs` — ANSI → Tallinn Night mapping (no changes needed)
- `terminal/input.rs` — Keystroke → escape sequence translation (no changes needed)
- `daemon_client.rs` — Async smol IPC client (used in Phase 4)
- `state/app_state.rs` — Session data, project filtering (read-only for terminal features)
- `watcher.rs` — File system watching for session changes (no changes for Phase 1-3)
- `components/status_indicator.rs` — Status dots with glow (reuse in tabs)

**gpui-component 0.5.1 — Available Components for Phase 2.5+**
- `resizable` module — `h_resizable()` / `v_resizable()`, `ResizablePanelGroup`, `ResizablePanel`, `ResizableState` with built-in resize handles and proportional sizing. **Use for Phase 2.8 pane grid resize handles (optional).** 2x2 grid starts with equal sizing; resizable panels add adjustable proportions. No custom mouse tracking needed.
- `kbd` module — `Kbd` widget renders platform-aware shortcut badges (⌘ on macOS, Ctrl on Windows). `Kbd::format()` for string rendering. **Use for Phase 2.7 status bar hints.**
- `sidebar` module — `SidebarHeader`, `SidebarGroup`, `SidebarMenu`, `SidebarFooter`. **Evaluate for Phase 2.6 navigation-only sidebar restructure.**
- `dock` module — `DockPanel`, `StackPanel`, `TabPanel`, `Tiles`. Complex panel management. **Evaluate if needed, but may be overkill.**
- `tab` module — Tab components. **Evaluate for terminal tab bar improvements.**
- Keyboard modifiers: `event.keystroke.modifiers.platform` = ⌘ on macOS. Already proven with Cmd+C/V in `terminal_view.rs`.

---

*Generated: 2026-02-11*
*Updated: 2026-02-12 — Spike 1 + Spike 2 validated. Both spikes complete.*
*Updated: 2026-02-12 — Rewritten to reflect bottom-up approach. Local PTY first, daemon opt-in, agent teams experimental. Simpler phasing (6 phases instead of 7), each phase independently usable.*
*Updated: 2026-02-12 — Phase 1 complete. Terminal in worktree working, code reviewed, error handling hardened.*
*Updated: 2026-02-13 — Phases 2.1–2.4 complete. Subphases 2.5–2.9 planned for layout, keyboard nav, splits, and polish.*
*Updated: 2026-02-13 — Research findings: gpui-component resizable panels, Cmd key support, Kbd widget. Conceptual hierarchy documented. Daemon terminal semantics clarified.*
*Updated: 2026-02-13 — Aligned with mockup-v2.html. Three-view architecture (Control/Dashboard/Detail). Sidebar navigation-only. 2x2 pane grid replaces split panes. Monochrome button discipline. Subphases 2.5–2.9 restructured.*
*Status: IN PROGRESS — Phase 2.5 complete, Phase 2.6 next*

---

## Phase 2 Subphase Detail (2.5–2.9)

Phases 2.1–2.4 delivered the core terminal multiplexer: click kild → terminal in worktree, persistence across switching, tabs, and daemon-backed terminals. Phases 2.5–2.9 complete the full multiplexer UX from the mockup (`mockup-v2.html`): project rail, navigation-only sidebar, three-view architecture (Control pane grid, Dashboard fleet cards, Kild Detail drill-down), keyboard navigation, and status bar.

```
Phase 2.4 (daemon terminals) ✅
  → Phase 2.5 (extract + keyboard nav) ✅
    → Phase 2.6 (rail + sidebar + view shell)
      → Phase 2.7 (dashboard + detail views)
      → Phase 2.8 (control view — pane grid)
    → Phase 2.9 (status bar + polish) ← depends on 2.6
```

### Phase 2.5: Extract + Keyboard Navigation
- **Goal**: Refactor `main_view.rs` (2251 lines) and add keyboard-driven kild navigation
- **Scope**:
  - Extract `TerminalTabs` + `TabEntry` + `TerminalBackend` + `render_tab_bar` + tab actions into `views/terminal_tabs.rs`
  - Introduce `FocusRegion` enum (`Sidebar | KildList | Terminal`) on `MainView` to track which area owns keyboard focus
  - ⌘J / ⌘K: navigate kild list selection up/down (requires `highlighted_index` state). Uses `modifiers.platform` for Cmd key.
  - ⌘1–9: jump to kild by index
  - Navigation includes all non-completed/non-destroyed kilds in current project filter (stopped kilds are navigable)
  - ⌘J/K works in both dashboard and terminal mode (blind switching like tmux — no sidebar needed for nav)
  - Mouse click still works as before — keyboard nav is additive
  - Aggressively clean up `main_view.rs` — extract logical sections into separate modules where it makes sense
- **Success signal**: Navigate and switch kilds entirely by keyboard. `main_view.rs` is significantly smaller and concerns are separated.
- **Key changes**: New `views/terminal_tabs.rs`, `views/main_view.rs` (add FocusRegion, key handlers, reduce size)

### Phase 2.6: Project Rail + Sidebar Restructure + View Shell
- **Goal**: Replace 200px sidebar + 3-column dashboard with mockup's Rail | Sidebar | Main layout with view tab bar
- **Scope**:
  - New `views/project_rail.rs` — 48px vertical strip with project icons (first letter), badge counts, "+" add, settings placeholder
  - Selected indicator: left pill accent (matching mockup)
  - Sidebar becomes navigation-only: project header + grouped kild list (Active / Stopped sections)
  - Sidebar shows terminals nested under each kild (matching the hierarchy: project → kild → terminal). Click a terminal in sidebar → opens it in main area.
  - Hover actions on kild rows: editor, stop/open buttons. No detail mode in sidebar.
  - Top tab bar in main area: Control / Dashboard tabs. ⌘D toggles between them.
  - Main area renders one of three views: Control (terminal panes), Dashboard (fleet cards), Detail (kild drill-down). Control and Dashboard are tab-switchable; Detail replaces Dashboard when drilling into a card.
  - Remove 3-column dashboard layout; layout is always: Rail | Sidebar | Main area. Sidebar is always visible.
  - Evaluate `gpui_component::sidebar` module (`SidebarHeader`, `SidebarGroup`, `SidebarMenu`, `SidebarFooter`) for the restructure
  - **Monochrome button discipline**: All buttons use frosted ice primary, border secondary, ghost, or danger-reveals-on-hover styles. Colors reserved exclusively for status indicators (aurora/copper/ember dots), agent names (kiri), and git stats (+/-).
- **Success signal**: Rail filters projects. Sidebar is navigation-only with hover actions. View tab bar switches Control/Dashboard. Layout matches mockup.
- **Key changes**: New `views/project_rail.rs`, restructure `views/sidebar.rs`, add `views/view_tabs.rs`, update `MainView::render()`

### Phase 2.7: Dashboard + Detail Views
- **Goal**: Fleet overview with scannable kild cards and kild detail drill-down
- **Scope**:
  - New `views/dashboard_view.rs` — fleet summary bar (active/stopped/terminal counts) + grid of kild cards
  - Each card: status dot, kild name, agent name, note, git stats (+/-/files), duration, PR number, terminal count
  - Stopped cards visually dimmed (reduced opacity)
  - Click dashboard card → Detail view in main area (replaces Dashboard, Dashboard tab stays active to show breadcrumb)
  - New `views/detail_view.rs` — kild drill-down with hero (name, status badge, duration), note, session info (agent, created, branch, runtime), git stats (insertions/deletions, uncommitted status, commits ahead), PR info, terminals list (click → open in Control), worktree path, action buttons (editor, copy path, stop, destroy)
  - Back button + Escape returns to Dashboard
  - Detail terminal rows: click → switch to Control view with that terminal focused
- **Success signal**: Dashboard shows all kilds as cards with fleet summary. Click card → full detail view. Back navigates correctly.
- **Key changes**: New `views/dashboard_view.rs`, new `views/detail_view.rs`, update `MainView` view routing

### Phase 2.8: Control View — Pane Grid
- **Goal**: 2x2 pane grid showing terminals from multiple kilds in the same project
- **Scope**:
  - New `views/pane_grid.rs` — 2x2 grid layout using CSS grid (or `gpui_component::resizable` for resize handles)
  - Each pane shows one terminal from any kild in the current project (cross-kild panes)
  - Pane header: status dot + kild name / terminal name + branch path. Hover reveals maximize/close buttons.
  - Focus routing: click pane to focus. Focused pane has subtle ice border.
  - Maximize: one pane goes full-area, other 3 hide but stay alive. Click to restore.
  - Close pane: removes terminal from grid, slot becomes empty with "drag terminal here" placeholder.
  - Default layout: first 4 active terminals auto-populate on project switch
  - Sidebar terminal click → opens in next empty pane slot (or replaces least-recently-focused pane if grid is full)
  - **Use `gpui_component::resizable`** for resize handles if pane proportions need to be adjustable. Start with equal 1fr sizing.
- **Success signal**: 4 terminals from different kilds visible simultaneously. Focus, maximize, close all work.
- **Key changes**: New `views/pane_grid.rs`, update `MainView` Control view rendering

### Phase 2.9: Status Bar + Polish
- **Goal**: Thin footer with alerts and context-aware keyboard shortcut hints
- **Scope**:
  - New `views/status_bar.rs` — footer spanning full width below main content (across sidebar + main)
  - Left side: contextual alerts (e.g., "fix-login-bug needs rebase")
  - Right side: keyboard shortcut hints that change based on active view:
    - Control: `⌘J/K nav`, `⌘D dashboard`, `⌘1-9 jump`
    - Dashboard: `⌘J/K nav`, `⌘D control`, `Enter detail`
    - Detail: `Esc back`, `⌘D control`
  - Use `gpui_component::kbd::Kbd` widget for platform-aware shortcut rendering (⌘ on macOS, Ctrl on Windows)
  - Wire into grid layout (`grid-template-rows: 1fr auto`)
  - Optional: minimized session bars below pane grid for background active kilds (could-have). Deferred if 2x2 grid + Dashboard provide sufficient fleet awareness.
- **Success signal**: Bar visible with contextual hints. Hints change when switching views.
- **Key changes**: New `views/status_bar.rs`, update `MainView::render()`
- **Note**: Minimized session bars are "Could have" in MoSCoW. The 2x2 pane grid (Phase 2.8) + Dashboard (Phase 2.7) may provide sufficient fleet awareness, making minimized bars unnecessary. Evaluate after 2.7 + 2.8 ship.
