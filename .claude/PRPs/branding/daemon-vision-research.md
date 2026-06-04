# KILD Daemon Vision: Research & Architecture

> Research conducted 2026-02-09. Covers issue #227, Opus 4.6 agent teams impact,
> and the KILD daemon + tmux-replacement architecture.

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Industry Context (Feb 2026)](#industry-context)
3. [Competitive Analysis: Agent Teams vs KILD](#competitive-analysis)
4. [Core Insight: Agent Sessions != Terminal Sessions](#core-insight)
5. [Architecture: KILD as tmux Replacement](#architecture)
6. [The tmux Shim: How It Works](#the-tmux-shim)
7. [The KILD Daemon](#the-kild-daemon)
8. [UI Decision: GPUI Multiplexer (not ratatui TUI)](#ui-decision)
9. [Component Library: gpui-component (Longbridge)](#component-library)
10. [Six Innovations Beyond tmux](#six-innovations)
11. [Agent Team Isolation Model](#agent-team-isolation-model)
12. [Claude Code Integration Surface](#claude-code-integration)
13. [Rust Crate Stack](#rust-crate-stack)
14. [Phased Delivery](#phased-delivery)
15. [Open Questions](#open-questions)
16. [References](#references)

---

## Problem Statement

### What KILD does today

KILD creates isolated git worktrees and launches AI agents (Claude, Kiro, Gemini, Codex, etc.)
in external terminal windows (Ghostty, iTerm, Terminal.app). Sessions are tracked as JSON files
on disk. Agent processes are "fire and forget" — KILD spawns a terminal and tracks the PID.

### What's changing in the industry

As of Feb 2026, coding agents are evolving from single-agent to multi-agent:

- **Claude Code Agent Teams** (released Feb 5, 2026): A lead session spawns teammate sessions
  that work in parallel, share a task list, and message each other. Uses tmux for split-pane
  display.
- **Context compaction + 1M token context**: Agents can now sustain much longer sessions.
- **Other agents** (Kiro, Gemini CLI, Codex) will follow with similar multi-agent features.

### The problem

1. **"Launch agent in terminal" is being commoditized.** Agent teams handle multi-agent
   coordination natively. KILD's current value of "spin up multiple agents" overlaps with
   what agent providers now offer built-in.

2. **KILD has no persistent session layer.** Sessions are JSON files + PID tracking. If the
   terminal dies, the session is gone. No reattach, no checkpoint, no recovery.

3. **KILD delegates to external terminals.** It has no visibility into what agents are doing
   beyond process-level monitoring. No structured state, no cross-session awareness.

4. **Agent teams are ephemeral and Claude-only.** They don't persist across restarts, can't
   be reattached, and only work with Claude Code. Multi-vendor agent fleets need an external
   orchestration layer.

### The opportunity

Build a **persistent session daemon** that:
- Replaces tmux as the session substrate for AI agents
- Owns PTYs and agent processes directly
- Is agent-agnostic (works with any CLI agent)
- Enables innovations impossible in tmux (checkpointing, forking, session graphs)
- Presents a tmux-compatible interface so existing tools (Claude Code agent teams) work
  without modification

This is KILD's differentiation: not the agent, not the terminal — the **session infrastructure**.

---

## Industry Context

### Claude Opus 4.6 (Feb 5, 2026)

Key capabilities relevant to KILD:
- **Agent Teams**: Multi-agent coordination with lead + teammates, shared task list, messaging
- **Context compaction**: Automatic context summarization for longer sessions
- **1M token context**: First Opus-class model with 1M window
- **128k output tokens**: Larger outputs per request
- **Adaptive thinking**: Model decides when to use extended thinking

### Agent Teams Technical Details

- Enabled via `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` env var
- Display modes: `in-process` (all in one terminal) or `tmux` (split panes)
- tmux detection: checks `$TMUX` environment variable exists
- Team config stored at `~/.claude/teams/{team-name}/config.json`
- Task list stored at `~/.claude/tasks/{team-name}/`
- Communication: automatic message delivery between teammates, shared task list
- Hooks: `TeammateIdle`, `TaskCompleted` for external tooling integration
- **Limitations**: No session resumption, no nested teams, Claude-only, split panes
  not supported in Ghostty

### What Agent Teams DON'T solve

| Gap | Detail |
|-----|--------|
| Git isolation | Teammates share one git context, can overwrite each other's files |
| Persistence | `/resume` doesn't restore teammates, lead dies = team dies |
| Agent-agnostic | Claude-only. No Kiro, Gemini, Codex support |
| PR/merge workflow | No concept of "work is done, check PR, merge" |
| Fleet operations | No `stats --all`, `sync --all`, health monitoring |
| Cross-session intelligence | Lead coordinates within one team only |

---

## Competitive Analysis

### Where KILD still wins (moat)

1. **Git isolation via worktrees** — Each agent gets a clean branch. No file conflicts.
   Agent teams warn about file conflicts; KILD prevents them architecturally.

2. **Persistence** — KILD sessions survive terminal death, can be reattached (with daemon).
   Agent teams are ephemeral.

3. **Agent-agnostic** — KILD runs Claude, Kiro, Gemini, Codex, Amp, OpenCode. Agent teams
   are Claude-only.

4. **PR/merge workflow** — `kild complete`, PR status checking, merge queue vision (#223).

5. **Fleet operations** — `kild stats --all`, `kild sync --all`, cross-kild health monitoring.

### Where KILD's value is shifting

The "launch agents in terminals" layer is commoditized. Durable value moves to:
- **Isolation infrastructure** (worktrees today, containers/VMs tomorrow)
- **Session lifecycle** (persist, reattach, monitor, checkpoint, fork)
- **Fleet orchestration** (cross-agent coordination, merge queue, health)
- **The Toryō role** (survey, decide, direct — from anywhere)

### Complementary, not competing

Agent teams and KILD can be complementary layers:
- KILD provides the isolation boundary (worktree) and session lifecycle
- Inside the worktree, an agent spawns an agent team for sub-task parallelism
- KILD monitors the team via hooks and file watching
- KILD persists the session state across agent team lifetimes

---

## Core Insight

### tmux is for human terminal sessions. KILD is for agent sessions.

| Dimension | Terminal Session (tmux) | Agent Session (KILD) |
|-----------|------------------------|----------------------|
| I/O | Raw byte stream + escape codes | Structured: tool calls, edits, searches, thinking |
| State | Terminal scrollback buffer | Multi-dimensional: conversation, task, git, env |
| Detach | Disconnect from PTY | Pause agent, serialize context, free resources |
| Resume | Reconnect to PTY | Restore context, reconnect agent, continue task |
| Identity | A running shell process | Isolated env + agent process + task state + git state |
| Intelligence | None — dumb pipe | Daemon understands what agent is doing |

**tmux is a terminal multiplexer. KILD should be a session orchestrator.**

The terminal is just one possible *view* into a session, not the session itself. The daemon
manages sessions; the TUI, kild-ui, and mobile clients are different views into the same
session state.

---

## Architecture

### Decision: Replace tmux, don't wrap it

**Rejected approach**: KILD manages tmux sessions, uses tmux as infrastructure.
- Pros: Less work initially.
- Cons: Bounded by tmux's capabilities. Can't checkpoint, fork, or build session graphs.
  tmux sessions die when the server is killed. tmux has no concept of agent state.

**Chosen approach**: KILD daemon owns PTYs directly, provides a tmux-compatible shim.
- Pros: Full control over session layer. Can innovate freely. Not bounded by tmux.
- Cons: Must build PTY management, terminal state parsing, TUI rendering.
- Rationale: The Rust crate ecosystem (`portable-pty`, `alacritty_terminal`, `ratatui`)
  makes this feasible. The innovation surface (checkpointing, forking, session graphs)
  is the entire product differentiation.

### High-level architecture

```
┌──────────────────────────────────────────────────┐
│                  KILD Daemon                      │
│                                                   │
│  ┌─────────────────────────────────────────────┐  │
│  │           PTY Manager                        │  │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐          │  │
│  │  │PTY 0│ │PTY 1│ │PTY 2│ │PTY 3│  ...     │  │
│  │  │lead │ │arch │ │impl │ │test │          │  │
│  │  └──┬──┘ └──┬──┘ └──┬──┘ └──┬──┘          │  │
│  └─────┼───────┼───────┼───────┼──────────────┘  │
│        │       │       │       │                  │
│  ┌─────┴───────┴───────┴───────┴──────────────┐  │
│  │           Session Engine                     │  │
│  │  • session graph (DAG)                      │  │
│  │  • checkpoints (git SHA + state snapshots)  │  │
│  │  • cross-session intelligence               │  │
│  │  • agent state (from hooks + file watching) │  │
│  └────────────────────────────────────────────┘  │
│                                                   │
│  ┌────────────────────────────────────────────┐  │
│  │           IPC Layer (unix socket)           │  │
│  └──────┬──────────┬──────────┬───────────────┘  │
└─────────┼──────────┼──────────┼──────────────────┘
          │          │          │
     ┌────┴───┐ ┌───┴────┐ ┌──┴──────┐
     │KILD TUI│ │kild-ui │ │tmux shim│
     │(ratatui│ │(GPUI)  │ │(for     │
     │ multi- │ │desktop │ │ Claude  │
     │ pane)  │ │app     │ │ Code)   │
     └────────┘ └────────┘ └─────────┘
```

Three clients to the same daemon:

1. **KILD TUI** — The primary interface. Agent-aware terminal multiplexer with multi-project
   navigation, pane management, notifications, and status bars. Built with `ratatui`.

2. **kild-ui (GPUI)** — Desktop dashboard. Fleet overview, merge readiness, session graphs.
   The Tōryō view. Embeds terminal rendering via `alacritty_terminal`.

3. **tmux shim** — Compatibility layer for agent tools. A `tmux` binary that translates
   tmux CLI commands into KILD daemon IPC. Claude Code thinks it's in tmux.

---

## The tmux Shim

### How it works

Claude Code detects tmux via the `$TMUX` environment variable. When set, it uses split-pane
mode and shells out to the `tmux` CLI to create/manage panes.

KILD exploits this:

1. Place a `kild-tmux-shim` binary on PATH (aliased/symlinked as `tmux`, ahead of real tmux)
2. Set `$TMUX=/tmp/kild-daemon.sock,<pid>,0` in the agent's environment
3. Claude Code detects "tmux", uses split-pane mode
4. All tmux commands hit KILD's shim instead of real tmux
5. Shim translates to KILD daemon IPC calls

### Commands to implement

Based on analysis of Claude Code's tmux integration, the shim needs ~10-15 commands:

```
tmux new-session         → daemon: create session
tmux split-window        → daemon: create pane, allocate PTY, spawn process
tmux send-keys           → daemon: write to PTY stdin
tmux select-pane         → daemon: set active pane
tmux list-panes          → daemon: query pane state
tmux list-sessions       → daemon: query sessions
tmux kill-pane           → daemon: close pane, kill process
tmux display-message     → daemon: log notification
tmux resize-pane         → daemon: resize PTY dimensions
tmux has-session         → daemon: session exists?
tmux set-option          → daemon: set session option (or no-op)
tmux set-environment     → daemon: set env var for session
```

The shim is estimated at ~500 lines of Rust. It parses tmux CLI args (using `clap`) and
makes IPC calls to the daemon.

### What we need to verify

The exact tmux commands Claude Code uses should be verified by reading Claude Code's source
(it's JavaScript on npm). The shim only needs to implement what's actually called.

**TODO**: Audit Claude Code source for tmux command usage.

---

## The KILD Daemon

### Responsibilities

1. **PTY ownership** — Allocate and manage pseudo-terminals for all agent processes.
   Processes are children of the daemon, not of any terminal. Survive terminal disconnects.

2. **Session lifecycle** — Create, pause, resume, checkpoint, fork, destroy sessions.
   Richer than tmux's attach/detach.

3. **Session engine** — Session graph (DAG), checkpoint storage, cross-session intelligence.

4. **Agent state tracking** — Receive structured state from hooks and file watching.
   Correlate across sessions. Detect conflicts, stuck agents, patterns.

5. **IPC server** — Unix socket server accepting connections from TUI, kild-ui, tmux shim,
   and (future) remote clients.

6. **Process management** — Spawn, monitor, signal agent processes. Health checking.

### Session lifecycle states

```
create → warm → run → pause → hibernate → resume → fork → complete
                 ↑            ↓
                 └── restart ──┘
```

Beyond tmux's binary attached/detached, KILD sessions have rich states:
- **warm**: environment created (worktree, deps installed), agent not yet started
- **run**: agent process active, PTY allocated
- **pause**: agent process suspended (SIGSTOP), PTY preserved, resources held
- **hibernate**: agent state serialized, PTY released, resources freed. Can resume later.
- **fork**: clone session state at current checkpoint, create parallel session

### Checkpointing

Periodic snapshots of session state:
- Git state: commit SHA, staged changes, working tree diff
- Agent task progress: task list state from `~/.claude/tasks/`
- Terminal scrollback: last N lines of PTY output
- Metadata: timestamp, agent name, session config

Checkpoints enable:
- **Rollback**: Agent went wrong direction → restore checkpoint, retry
- **Fork**: Try two approaches from same checkpoint in parallel
- **Recovery**: Daemon crash → restore from last checkpoint
- **Audit**: Full history of what each agent did and when

### Session graph (DAG)

Sessions can have dependencies and data flow:

```
A (research) ──→ B (backend) ──┐
                                ├──→ D (integrate) ──→ E (review)
               → C (frontend) ─┘
```

The daemon manages the graph: auto-starts downstream sessions when upstream completes,
passes structured context between sessions, detects cycles.

This generalizes the merge queue (#223) — the queue is one type of session graph.

### Cross-session intelligence

The daemon has a global view across all sessions:
- **Conflict detection**: Session B editing `auth.rs`, Session C about to edit `auth.rs` → warn
- **Knowledge sharing**: Session A discovered a pattern → inject into Session B's context
- **Fleet patterns**: 3/5 sessions stuck on same test → the test itself might be broken

---

## UI Decision: GPUI Multiplexer (not ratatui TUI)

### Decision: Build the multiplexer in GPUI, not as a ratatui TUI

**Decision date**: 2026-02-09

### Reasoning

The question was whether to build a terminal-based TUI (ratatui + crossterm) or a native
GPUI desktop application for the multiplexer interface. GPUI wins decisively:

| Dimension | GPUI Multiplexer | ratatui TUI |
|-----------|------------------|-------------|
| **Existing code** | 7,500 lines of production kild-ui to build on | Start from scratch |
| **Terminal rendering** | GPU-accelerated, 60-120 FPS, native fonts | Constrained by host terminal |
| **Mixed content** | Terminal pane + status bar + notifications + dashboard, all composable | Everything is a text grid |
| **Tool call rendering** | Syntax-highlighted cards, collapsible sections | Parse and reformat as ANSI text |
| **Agent state viz** | Custom widgets, progress bars, animated status | Limited to text + box drawing chars |
| **Multi-project nav** | Tabs, sidebar, split views — native UX patterns | Manual key bindings, no mouse |
| **Session graph (future)** | Interactive DAG visualization with GPUI drawing | Not practically possible |
| **Works over SSH** | No | Yes |

The only ratatui advantage is SSH access. But KILD is a local developer tool, and the
daemon IPC model means a lightweight TUI client could exist later as a secondary interface.
The primary interface should be the best possible experience.

**Key factor: kild-ui already exists.** It has event-driven state management, a complete
theme system (Tallinn Night), a component library, file watching, background executors,
and comprehensive error handling. Adding terminal rendering is additive — not a rewrite.

### kild-ui current state (what already exists)

~7,500 lines of production-ready GPUI code:

**State management** (`state/` — 1,404 lines):
- `app_state.rs` — Main facade with `apply_events()` for event-driven updates
- `sessions.rs` — Session display data with status tracking
- `dialog.rs` — Dialog state (Create, Confirm, AddProject)
- `errors.rs` — Per-branch and bulk operation error tracking
- `loading.rs` — In-progress operation tracking (prevents double-dispatch)
- `selection.rs` — Kild selection state for detail panel

**Views** (`views/` — 2,969 lines):
- `main_view.rs` — Root view with Rail | Sidebar | Main | StatusBar layout
- `sidebar.rs` — 200px kild navigation grouped by Active/Stopped
- `dashboard_view.rs` — Fleet overview with kild cards
- `detail_view.rs` — Kild drill-down from dashboard cards
- `status_bar.rs` — Contextual alerts and view-aware keyboard hints
- `create_dialog.rs`, `confirm_dialog.rs`, `add_project_dialog.rs` — Modal dialogs

**Components** (`components/`):
- `button.rs` — Themed button with 6 variants (Primary, Secondary, Ghost, Success, Warning, Danger)
- `status_indicator.rs` — Status dots with glow effects (Active/Stopped/Crashed)
- `text_input.rs` — Styled text input with cursor
- `modal.rs` — Modal container with overlay

**Infrastructure**:
- `theme.rs` — Tallinn Night color system (full palette, typography scale, spacing constants)
- `watcher.rs` — File system watcher for instant refresh (notify crate, 100ms debounce)
- `refresh.rs` — Hybrid file watching + 60s slow poll fallback
- `actions.rs` — Business logic handlers dispatching through CoreStore

**What it does NOT have**: Terminal rendering. It delegates to Ghostty/iTerm/Terminal.app
via kild-core's TerminalBackend trait. Every kild opens an external window.

### GPUI terminal rendering (proven pattern)

Zed editor does this in production. Three community projects (gpui-terminal, zTerm,
gpui-ghostty) have independently validated the approach.

**The rendering pipeline:**

```
PTY (agent process stdout)
    ↓ raw bytes
alacritty_terminal::Term
    ↓ VTE parsing → grid of cells (chars, colors, attributes)
TerminalContent (renderable snapshot)
    ↓ GPUI Element trait
TerminalElement
    ↓ prepaint() → layout, paint() → GPU draw calls
GPUI Renderer (Metal on macOS)
    ↓
Screen at 60-120 FPS
```

`alacritty_terminal` is the key crate. It handles all VT100/ANSI escape sequence parsing
and maintains the terminal grid state (cells, colors, scrollback, selection, cursor).
Zed wraps it in a Terminal struct, extracts renderable content via `make_content()`,
and paints it through a GPUI Element.

**Performance techniques (from Zed)**:
- 4ms event batching (coalesce PTY events before redrawing)
- Viewport clipping (only render visible rows)
- Text run batching (adjacent cells with same style → single draw call)
- Background region merging (contiguous colored regions → single rectangle)

**Community validation**:
- `gpui-terminal` (zortax): Generic Read/Write streams + alacritty_terminal, 16/256/24-bit color
- `zTerm` (zerx-lab): Full terminal emulator on GPUI, encountered and solved batching issues
- `gpui-ghostty` (Xuanwo): Integrates Ghostty's libghostty-vt with GPUI rendering

### What changes in kild-ui

**Keep (existing foundation)**:
- `state/` — Event-driven state management (add terminal state alongside session state)
- `theme.rs` — Tallinn Night palette (extend with ANSI 16 colors, cursor, selection colors)
- `components/` — Button, StatusIndicator, Modal, TextInput (all reusable as-is)
- `watcher.rs` — File watching for session changes (unchanged)
- `actions.rs` — Business logic handlers (modify to create PTY instead of spawning external terminal)
- `views/sidebar.rs` — Project navigation (extend with agent/teammate tree)
- `views/main_view.rs` — Layout shell (evolve from 3-column dashboard to multiplexer layout)

**Add (new capabilities)**:
- `terminal/terminal.rs` — Terminal state wrapping `alacritty_terminal::Term`, event buffering
- `terminal/terminal_element.rs` — GPUI Element implementation for rendering terminal grid
- `terminal/mod.rs` — Module organization
- `views/terminal_pane.rs` — View composing terminal element + status bar + teammate tabs
- `views/minimized_session.rs` — Collapsed session widget with agent state summary
- `views/notification_bar.rs` — Bottom bar for cross-session alerts
- `daemon_client.rs` — IPC client connecting to KILD daemon (replaces direct kild-core calls)

**Modify (evolve existing)**:
- `actions.rs` — Instead of `Command::OpenKild` spawning Ghostty, create a PTY via daemon IPC
- `views/sidebar.rs` — Navigation-only kild list with nested terminal tabs (Active/Stopped grouping)
- `views/detail_view.rs` — Full kild drill-down with terminal list (click → opens in Control view)
- `views/dashboard_view.rs` — Fleet cards with git stats, PR info, terminal counts
- `views/pane_grid.rs` — 2x2 terminal grid for cross-kild terminal viewing
- `views/status_bar.rs` — Contextual alerts and keyboard shortcut hints

**New dependencies**:
```toml
alacritty_terminal = "0.25"    # Terminal emulation engine
portable-pty = "0.8"           # PTY management (direct or via daemon IPC)
parking_lot = "0.12"           # FairMutex for terminal state (Zed pattern)
```

### Layout concept

```
┌─ KILD ───────────────────────────────────────────────────────┐
│ [my-app ▾] [other-repo]                    ⚡ 4 agents   ⚙  │
├──────────────┬───────────────────────────────────────────────┤
│              │ ┌─ feature-auth (claude team) ─── 3/5 ────┐  │
│ feature-auth │ │                                          │  │
│  ├ lead   🟢 │ │  [GPUI-rendered terminal output]        │  │
│  ├ arch   🟢 │ │  $ I'll create the JWT middleware...    │  │
│  ├ impl   🔵 │ │                                          │  │
│  └ test   ⏳ │ │  ┌─ Edit: src/auth/middleware.rs ─────┐ │  │
│              │ │  │ + pub fn validate(token: &str) {    │ │  │
│ feature-api  │ │  │ +     decode(token, &key, &val)     │ │  │
│  └ kiro   🟢 │ │  │ + }                                │ │  │
│              │ │  └─────────────────────────────────────┘ │  │
│ bugfix-auth  │ │                                          │  │
│  └ claude 🟡 │ ├─ [lead] [architect] [implementer] [test]┤  │
│              │ └──────────────────────────────────────────┘  │
│              │                                               │
│              │ ┌─ feature-api (kiro) ── minimized ────────┐  │
│              │ │ 🟢 editing src/api/routes.rs (2m ago)    │  │
│              │ └──────────────────────────────────────────┘  │
│              │                                               │
│              │ ┌─ bugfix-auth ── minimized ────────────────┐  │
│              │ │ 🟡 "Add a migration?" — waiting (5m)     │  │
│              │ └──────────────────────────────────────────┘  │
├──────────────┴───────────────────────────────────────────────┤
│ ⚠ bugfix-auth needs input (5m)  ✓ arch completed task (8m)  │
└──────────────────────────────────────────────────────────────┘
```

Key UX features only possible in GPUI (not ratatui):
- **GPU-rendered terminal pane** with native font rendering (Core Text on macOS)
- **Tool call cards** — syntax-highlighted, collapsible, inline with terminal output
- **Minimized sessions** — collapsed to single line with agent state summary
- **Teammate tab bar** — click to switch between lead/architect/implementer/tester PTYs
- **Notification bar** — styled overlay with contextual alerts
- **Mixed content** — terminal pane alongside status widgets, progress indicators, buttons

### Key differences from tmux

| Feature | tmux | KILD GPUI Multiplexer |
|---------|------|-----------------------|
| Rendering | Terminal escape sequences | GPU-accelerated (Metal/Vulkan), 60-120 FPS |
| Minimized panes | Not possible, panes are fixed grid | Collapsed with status summary |
| Agent state | Not shown | Thinking / editing / stuck / idle (custom widgets) |
| Tool calls | Raw terminal output | Rendered as syntax-highlighted cards |
| Notifications | Bell character | Contextual: "agent needs input", "task completed" |
| Navigation | Pane index, window number | Project > kild > agent hierarchy |
| Task progress | Not tracked | "3/5 tasks, 67% ready" per session |
| Multi-project | Separate sessions | First-class project tabs |
| Font rendering | Host terminal font | Platform-native (Core Text on macOS) |
| Mouse support | Limited | Full native mouse, click, drag, scroll |
| Theming | 256 color terminal | Full RGB, gradients, transparency, glow effects |

### Daemon connection

kild-ui connects to the daemon via IPC, not directly to PTYs:

```
KILD Daemon (background process, owns PTYs)
    ↑ IPC (unix socket)
    │
    ├── kild-ui (GPUI) connects, receives:
    │   • PTY output streams (bytes → alacritty_terminal → render in GPUI)
    │   • Session state updates (agent status, task progress, overlap alerts)
    │   • Notifications (agent stuck, task completed, overlap detected)
    │
    ├── tmux shim connects (for Claude Code agent teams)
    │   • Creates PTYs via daemon IPC
    │   • kild-ui sees new panes appear automatically
    │
    └── kild CLI connects (for quick terminal operations)
        • kild list, kild stats, kild overlaps
```

Key behaviors:
- Close kild-ui → agents keep running (daemon owns processes)
- Reopen kild-ui → reconnect to daemon, see all running sessions immediately
- Multiple kild-ui windows → all showing same daemon state
- kild-ui doesn't manage PTYs directly — it renders PTY output from the daemon

### Development path for kild-ui evolution

**Phase 1a (standalone spike)**: Build terminal rendering component in kild-ui using
`alacritty_terminal` + GPUI Element trait. Get a single terminal pane rendering correctly.
Use `portable-pty` directly for testing. No daemon dependency yet.

**Phase 1b (daemon integration)**: Connect kild-ui to daemon IPC. Replace direct PTY
management with daemon streams. Terminal panes now render daemon-owned PTY output.

**Phase 2 (multiplexer UX)**: Multi-pane layout. Teammate tabs within kild terminal views.
Minimized sessions with agent state. Notification bar. tmux shim creating panes that
kild-ui renders.

**Phase 3 (agent-aware rendering)**: Tool call cards. Structured agent state widgets.
Session graph visualization. Full Tōryō dashboard mode.

---

## Component Library: gpui-component (Longbridge)

### Decision: Use Longbridge's gpui-component library for UI components

**Decision date**: 2026-02-09

### Why a component library

The current kild-ui has hand-built components (Button, TextInput, Modal, StatusIndicator).
They work but lack:
- **Accessibility**: No screen reader support, no ARIA labels
- **Copy/paste**: TextInput doesn't support clipboard operations properly
- **Selection**: No text selection in inputs
- **IME**: No input method editor support (CJK languages)
- **Component coverage**: Missing tabs, tables, trees, virtual lists, resizable panels,
  command palette, context menus, toast/notifications, dropdowns, etc.

Building all of this from scratch would take months. A component library provides
production-quality fundamentals so we can focus on the unique parts (terminal rendering,
daemon integration, agent-aware UI).

### The two contenders

| Aspect | gpui-component (Longbridge) | adabraka-ui |
|--------|----------------------------|-------------|
| **GitHub stars** | 10,270 | 345 |
| **Latest version** | 0.5.1 (Feb 5, 2026) | 0.3.0 (Feb 5, 2026) |
| **Components** | 60+ | 80+ |
| **Production apps** | Longbridge Pro (trading desktop) | Demos only |
| **Accessibility** | Partial (keyboard nav, unclear screen reader) | Full (ARIA, screen reader, keyboard) |
| **Advanced features** | LSP editor (200K lines), virtualized tables, charts | CommandPalette, animations |
| **Theming** | Full system with ThemeColor | Full system with semantic tokens |
| **License** | Apache 2.0 | MIT |
| **Rust nightly** | Not required | Required |
| **Maturity** | Battle-tested in production | Newer, unproven at scale |

### Why Longbridge wins

1. **Production-proven.** Used in Longbridge Pro, a real trading desktop app. adabraka-ui
   has only demo apps. For KILD's multiplexer — where reliability matters (you're watching
   agent output, managing sessions) — production pedigree is more important than component
   count.

2. **The components we need most are their strongest:**
   - **Tabs** — for terminal session tabs (exactly like the mockup-embedded.html design)
   - **Sidebar** — for the kild list with project grouping
   - **Resizable** — for adjustable panel widths (sidebar ↔ terminal)
   - **Table** — for fleet overview (`kild stats --all` in the GUI)
   - **Tree** — for agent/teammate hierarchy within a kild
   - **VirtualList** — for scrolling through 30+ kilds efficiently
   - **Toast/Notification** — for agent alerts ("needs input", "task completed")
   - **Dialog/Modal** — for create kild, confirm destroy, settings
   - **Menu** — for context menus on kild items (right-click → stop/destroy/open)
   - **Input/Textarea/Select** — proper text inputs with copy/paste/selection

3. **The Editor component is a strategic bonus.** 200K-line capable code editor with LSP
   support, syntax highlighting via Tree-sitter. If we want to show inline diffs, code
   previews, or tool call details in the multiplexer — it's there.

4. **No Rust nightly requirement.** adabraka-ui requires nightly, which adds build
   complexity and potential instability. Longbridge works on stable Rust.

5. **Charts.** For the Tōryō dashboard view (Phase 3) — agent activity over time, resource
   usage, task completion rates. Longbridge has Line, Bar, Area, Pie, Candlestick charts.
   Not critical now, but nice to have in the library.

### Accessibility pragmatism

adabraka-ui has better accessibility claims. But GPUI itself has incomplete accessibility —
Zed's own a11y is still work-in-progress (beyond Zed 1.0 timeline). Until GPUI's
framework-level accessibility matures (focus management, screen reader bridges), no
component library can fully solve it. Use Longbridge now for speed, invest in
accessibility as GPUI improves.

### Theming: Tallinn Night → Longbridge ThemeColor

Longbridge uses a `ThemeColor` system with semantic tokens. The Tallinn Night brand palette
maps directly:

```
Tallinn Night               →  Longbridge ThemeColor equivalent
─────────────────────────────────────────────────────────────
Void (#08090A)              →  background (deepest)
Obsidian (#0E1012)          →  panel / sidebar background
Surface (#151719)           →  card / elevated surface
Elevated (#1C1F22)          →  modal / dropdown / popover

Border Subtle (#1F2328)     →  border (subtle)
Border (#2D3139)            →  border (default)
Border Strong (#3D434D)     →  border (emphasis)

Text Muted (#5C6370)        →  foreground_disabled
Text Subtle (#848D9C)       →  foreground_muted
Text (#B8C0CC)              →  foreground
Text Bright (#E8ECF0)       →  foreground_emphasis
Text White (#F8FAFC)        →  foreground_max

Ice (#38BDF8)               →  accent / primary
Ice Dim (#0EA5E9)           →  accent_active
Ice Bright (#7DD3FC)        →  accent_hover

Aurora (#34D399)            →  success
Copper (#FBBF24)            →  warning
Ember (#F87171)             →  danger / error
Kiri (#A78BFA)              →  info (repurposed for AI activity)
```

Longbridge supports custom theme files in a `themes/` directory. We create a
`tallinn-night.json` theme that maps our brand tokens to their system. The existing
brand system (`.claude/PRPs/branding/brand-system.html`) documents all values.

Typography maps directly: Inter for UI (`--font-ui`), JetBrains Mono for code
(`--font-mono`). These are the same fonts the brand system specifies.

### Component mapping to multiplexer UI

Based on the `mockup-embedded.html` design from the brand research:

```
┌─ Header ─────────────────────────────────────────────────────┐
│ [Logo] [Stats]                        [Actions: + New Kild]  │  ← Longbridge: Button, Badge
├──────────────┬───────────────────────────────────────────────┤
│              │ ┌─ Tab Bar ────────────────────────────────┐  │  ← Longbridge: Tabs
│  Sidebar     │ │ [feature-auth] [feature-api] [bugfix] [+]│  │
│  (kild list) │ └──────────────────────────────────────────┘  │
│              │ ┌─ Terminal Header ─────────────────────────┐  │  ← Longbridge: various
│  Longbridge: │ │ [name] [path]          [actions: ⏹ Stop] │  │
│  Sidebar     │ └──────────────────────────────────────────┘  │
│  Tree        │ ┌─ Terminal Pane ──────────────────────────┐  │
│  VirtualList │ │                                          │  │  ← CUSTOM: TerminalElement
│              │ │  [alacritty_terminal grid rendered       │  │     (alacritty_terminal +
│              │ │   via GPUI Element trait]                │  │      GPUI Element trait)
│              │ │                                          │  │
│              │ └──────────────────────────────────────────┘  │
├──────────────┴───────────────────────────────────────────────┤
│ Notification Bar: [alerts]     Shortcuts: [⌘1-9] [⌘T] [⌘W] │  ← Longbridge: Toast, Kbd
└──────────────────────────────────────────────────────────────┘
```

| UI Area | Longbridge Components | Custom Code |
|---------|----------------------|-------------|
| Header | Button, Badge, Icon | Layout |
| Sidebar (kild list) | Sidebar, Tree, VirtualList | Agent status indicators |
| Tab bar | Tabs | Tab status dots (reuse StatusIndicator) |
| Terminal header | Button (icon), Badge | Layout, path display |
| Terminal pane | — | **TerminalElement** (custom GPUI Element) |
| Create dialog | Dialog, Input, Select, Button | Form validation logic |
| Confirm dialog | Dialog, Button | — |
| Context menu | Menu | Kild-specific actions |
| Notifications | Toast/Notification | Agent-aware messages |
| Keyboard hints | Kbd | Layout |
| Fleet dashboard (Phase 3) | Table, Chart, Badge | Merge readiness logic |
| Settings | Dialog, Input, Select, Toggle | Config integration |

### What we keep from current kild-ui

- **State management** (`state/`): Event-driven pattern stays. Works independently of UI components.
- **Watcher** (`watcher.rs`): File system watching logic stays.
- **Actions** (`actions.rs`): Business logic dispatch stays.
- **Theme values**: Tallinn Night colors migrate to Longbridge theme file, but semantic meaning stays.

### What we replace

- **Button component** → Longbridge Button (gains accessibility, proper disabled states)
- **TextInput component** → Longbridge Input (gains copy/paste, selection, IME)
- **Modal component** → Longbridge Dialog (gains focus trapping, escape handling, overlay)
- **StatusIndicator** → Keep as custom component (agent-specific semantics, glow effects)

### What we add from Longbridge

- Tabs, Sidebar, Tree, VirtualList, Table, Menu, Toast, Kbd, Select, Textarea,
  Resizable, Chart (Phase 3), Editor (Phase 3)

### Not locked in

The approach is modular. Longbridge components are used individually — we import `Button`
from Longbridge, not a monolithic framework. If a component doesn't meet needs later,
we replace just that component with a custom one. The Tallinn Night theme is the unifying
layer, not the component library.

If adabraka-ui matures and Longbridge stalls, migration is component-by-component, not
all-or-nothing. Same if GPUI's own component ecosystem evolves.

---

## Six Innovations

### 1. Structured Agent Protocol

The daemon understands agent state through hooks + file watching, not terminal parsing.
It knows: thinking, executing tool X, waiting for permission, editing file Y, tests failing.

Enables smart monitoring, automated intervention, structured logging, rich UI rendering.

### 2. Session Checkpointing & Forking

Periodic snapshots of full session state (git + tasks + scrollback). Enables rollback,
forking (try two approaches in parallel), recovery (daemon crash → restore), audit trail.

No one does this. tmux can't (no state understanding). Agent teams can't (ephemeral).

### 3. Session Graph (DAG)

Sessions have dependencies and data flow. Daemon auto-starts downstream sessions when
upstream completes. Generalizes merge queue. Enables complex multi-stage workflows.

### 4. Inter-Session Intelligence

Cross-session conflict detection, knowledge sharing, fleet-level pattern detection.
The daemon's global view enables insights no individual session can produce.

**Foundation already built**: PR #292 (`kild overlaps`) implements `collect_file_overlaps()`
in kild-core — a pure library function that detects when multiple kilds modify the same files.
In the daemon, this becomes continuous and proactive rather than CLI-triggered:

| Today (PR #292) | Daemon future |
|---|---|
| User runs `kild overlaps` manually | Daemon runs overlap check on every file change |
| Gets a report of overlapping files | TUI shows real-time overlap warnings |
| User decides what to do | Daemon can warn agents via hooks before conflicts happen |

The `OverlapReport` and `FileOverlap` types are already serializable (JSON), ready for
daemon IPC. The function handles partial failures gracefully — if one kild's repo can't be
opened, others still get analyzed. This resilience pattern is exactly what a daemon needs.

### 5. Session Templates & Replay

Codify workflows as reusable templates. Run the same workflow on different branches.
Infrastructure-as-code for agent work.

### 6. Portable Sessions

Serialize and move sessions between machines. Local → cloud, cloud → local. Enables
remote monitoring (mobile Tōryō vision from #227).

---

## Agent Team Isolation Model: One Kild Per Team

### The key abstraction

Each kild is an **isolation boundary**. Inside a kild, an agent can spawn an agent team
for intra-task parallelism. KILD doesn't manage teammates — it manages kilds. Agent teams
manage themselves within the isolation boundary KILD provides.

```
KILD Fleet (daemon manages this level)
├── kild: feature-auth (worktree: ~/.kild/worktrees/project/feature-auth)
│   └── Claude agent team (managed by Claude Code inside the kild)
│       ├── lead: coordinates auth feature
│       ├── architect: designs auth flow
│       ├── implementer: writes code
│       └── tester: writes tests
├── kild: feature-api (worktree: ~/.kild/worktrees/project/feature-api)
│   └── Claude agent team (lead + 2 teammates)
│       ├── lead: coordinates API work
│       ├── backend: implements endpoints
│       └── tester: writes tests
├── kild: bugfix-perf (worktree: ~/.kild/worktrees/project/bugfix-perf)
│   └── Single Kiro agent (no team)
└── kild: refactor-db (worktree: ~/.kild/worktrees/project/refactor-db)
    └── Single Claude agent (no team)
```

### Why this works

1. **Git isolation is per-kild, not per-teammate.** Teammates within an agent team share
   the same worktree — they CAN step on each other's files (the agent team docs warn about
   this). But between kilds, isolation is absolute: separate worktrees, separate branches.

2. **`kild overlaps` works at the right level.** It detects cross-kild file conflicts,
   which are the dangerous ones (separate branches that'll need merging). Intra-kild
   conflicts are the agent team's problem to manage.

3. **KILD is agent-agnostic.** Some kilds run Claude with agent teams. Some run single
   Claude. Some run Kiro. Some run Gemini. KILD doesn't care — it provides the isolation
   and lifecycle; the agent handles its own internal parallelism.

4. **The tmux shim enables agent teams transparently.** KILD sets `$TMUX`, provides the
   tmux shim. If Claude Code decides to spawn an agent team, the teammates get their own
   PTYs managed by KILD's daemon. KILD sees each teammate as a pane within the kild's
   session. Zero configuration needed from the user.

### Fleet orchestration path

This model opens the door to a future **agent team orchestrator**:

- **Fleet awareness**: KILD knows all kilds, their status, their overlaps, their health
- **Team awareness**: Via hooks + file watching, KILD knows teammate state within each kild
- **Cross-team coordination**: "feature-auth team's architect found the auth pattern uses JWT.
  Inject this knowledge into feature-api team's context." This is cross-kild intelligence
  that no individual agent team can achieve.
- **Resource management**: 4 kilds with 4 teammates each = 16+ Claude instances. KILD can
  throttle, prioritize, or queue kilds based on system resources or API rate limits.
- **Merge orchestration**: When feature-auth team completes, KILD can auto-merge, detect
  impacts on feature-api, and notify that team to rebase.

### Existing code that supports this

| Existing feature | Daemon role |
|---|---|
| `kild create` (git isolation) | Daemon creates worktree, allocates PTYs for agent + teammates |
| `kild overlaps` (PR #292) | Daemon runs continuously, warns before conflicts |
| `kild stats --all` (fleet health) | Daemon serves real-time health to TUI |
| `kild sync --all` (fleet rebase) | Daemon auto-syncs on schedule or trigger |
| Agent status sidecar | Daemon receives structured agent state via hooks |
| Session lifecycle (create/stop/destroy/complete) | Daemon manages full lifecycle per kild |

---

## Claude Code Integration Surface

### No Anthropic cooperation needed

Claude Code's integration with external tooling uses:

1. **`$TMUX` env var** — Set by KILD to trigger tmux mode. Claude Code checks this exists.
2. **`tmux` CLI** — Claude Code shells out to tmux commands. KILD's shim intercepts.
3. **Hooks** — Structured lifecycle events, configured in settings.json.
4. **File-based state** — Team configs and task lists at known paths.

### Hooks (primary structured channel)

Configured in `~/.claude/settings.json` or `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [{
      "hooks": [{ "type": "command", "command": "kild hook session-start" }]
    }],
    "TeammateIdle": [{
      "hooks": [{ "type": "command", "command": "kild hook teammate-idle" }]
    }],
    "TaskCompleted": [{
      "hooks": [{ "type": "command", "command": "kild hook task-completed" }]
    }],
    "Stop": [{
      "hooks": [{ "type": "command", "command": "kild hook agent-stop" }]
    }]
  }
}
```

Hooks receive JSON on stdin:

```json
{
  "session_id": "abc123",
  "transcript_path": "/path/to/session.jsonl",
  "cwd": "/working/directory",
  "hook_event_name": "TeammateIdle",
  "teammate_name": "researcher",
  "team_name": "my-project"
}
```

Key behaviors:
- **Exit 0**: Allow the action
- **Exit 2**: Block the action, stderr sent as feedback to agent
- **`SessionStart`**: Can inject env vars via `$CLAUDE_ENV_FILE` and context via `additionalContext`
- **`TeammateIdle`**: Can prevent teammate from going idle (exit 2 = keep working)
- **`TaskCompleted`**: Can prevent task completion (exit 2 = quality gate)

### File watching

KILD daemon watches:
- `~/.claude/teams/{team-name}/config.json` — team membership changes
- `~/.claude/tasks/{team-name}/` — task list state changes
- Session transcript files — agent activity

### Agent teams env vars

| Variable | Purpose |
|----------|---------|
| `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` | Enable agent teams (`1`) |
| `TMUX` | tmux detection (KILD sets this to its socket) |
| `CLAUDE_CODE_TEAM_NAME` | Team name for the session |

### What would benefit from Anthropic cooperation (not required)

1. **Custom `teammateMode: "kild"`** — Delegate session management entirely to KILD
2. **Structured agent state stream** — Real-time tool call, thinking state (beyond hooks)
3. **Protocol standardization** — If KILD defines the external session manager interface,
   other agents could adopt it

**Strategy**: Build Phase 1 without cooperation. Prove value. Approach Anthropic with
working product and protocol proposal.

---

## Rust Crate Stack

### Daemon crate (`kild-daemon`)

| Component | Crate | Notes |
|-----------|-------|-------|
| PTY management | `portable-pty` | Cross-platform PTY creation, used by Wezterm |
| Async runtime | `tokio` | Daemon IPC server, file watching, process management |
| IPC server | `tokio` unix sockets + serde JSON | Simple, debuggable, extensible to TCP later |
| File watching | `notify` | Already used by kild-core (watcher) |
| Process mgmt | `nix` / `libc` | Signal handling, process groups |

### tmux shim (`kild-tmux-shim`)

| Component | Crate | Notes |
|-----------|-------|-------|
| CLI parsing | `clap` | Parse tmux CLI args |
| IPC client | `tokio` unix socket + serde JSON | Connects to daemon |

### GPUI multiplexer (evolving `kild-ui`)

| Component | Crate | Notes |
|-----------|-------|-------|
| UI framework | `gpui` | Already in kild-ui, GPU-accelerated native UI |
| UI components | `gpui-component` | Longbridge library: Tabs, Sidebar, Dialog, Input, etc. |
| Terminal emulation | `alacritty_terminal` | VTE parsing + grid state, used by Zed |
| Terminal state lock | `parking_lot` | FairMutex for thread-safe terminal access (Zed pattern) |
| Business logic | `kild-core` | Already in kild-ui |
| File watching | `notify` | Already in kild-ui |
| IPC client | `tokio` unix socket | Connects to daemon (replaces direct kild-core calls) |

### Why `alacritty_terminal`

This is the terminal emulation core extracted from Alacritty. Zed editor uses it for their
integrated terminal. Three community GPUI projects also use it. It provides:
- Full VT100/ANSI escape sequence parsing
- Terminal grid state management (cells, colors, attributes)
- Scrollback buffer
- Cursor state and selection
- Resize handling

KILD feeds PTY bytes into `alacritty_terminal`, which maintains the terminal state grid.
kild-ui implements GPUI's Element trait to paint the grid via GPU. This is the same
architecture Zed uses for its integrated terminal — battle-tested and handles all edge
cases of terminal emulation.

### Why NOT `ratatui`

The decision was made to build the multiplexer in GPUI, not as a ratatui TUI. See
[UI Decision](#ui-decision) for full reasoning. In short: kild-ui already has 7,500 lines
of production GPUI code, GPUI enables mixed terminal + dashboard content that ratatui can't,
and the terminal rendering pattern is proven by Zed and community projects. A lightweight
ratatui-based `kild attach` CLI could exist as a secondary client in the future.

---

## Phased Delivery

### Phase 1a: GPUI terminal rendering spike

**Goal**: Prove terminal rendering works in kild-ui. Self-contained, no daemon dependency.

**Deliverables**:
- `terminal/` module in kild-ui: wraps `alacritty_terminal::Term`
- `TerminalElement`: GPUI Element trait implementation for rendering terminal grid
- Single terminal pane in kild-ui rendering a shell session (using `portable-pty` directly)
- Keyboard input routing (keystrokes → PTY stdin)
- 4ms event batching, viewport clipping, text run batching (Zed patterns)
- ANSI 16 colors mapped to Tallinn Night theme

**Value**: Validates the core technical risk. Once a terminal renders in GPUI, everything
else is UI composition.

### Phase 1b: Daemon + tmux shim

**Goal**: KILD daemon owns PTYs. Agents run inside KILD instead of external terminals.

**Deliverables**:
- `kild-daemon` crate: PTY management, session lifecycle, IPC server (unix socket)
- `kild-tmux-shim` binary: tmux command translation to daemon IPC (~15 commands)
- kild-ui connects to daemon IPC (replaces direct PTY management from Phase 1a)
- Hook integration: `SessionStart`, `TeammateIdle`, `TaskCompleted`, `Stop`
- `kild create` launches agents inside daemon instead of external terminal
- `kild attach <branch>` opens kild-ui connected to running session
- Claude Code agent teams work inside KILD via tmux shim (teammates get daemon PTYs)

**Value**: Persistent sessions that survive kild-ui closing. Agent teams transparently
use KILD. Basic multi-session management.

### Phase 2: Multiplexer UX + notifications

**Goal**: kild-ui becomes a full multiplexer with agent-aware features.

**Deliverables**:
- Multi-pane layout: expanded terminal view + minimized session cards
- Teammate tabs: switch between lead/architect/implementer/tester PTYs within a kild
- Minimized sessions: collapsed view showing agent state summary
- Notification bar: contextual alerts ("agent needs input", "task completed", "overlap detected")
- Agent state tracking: thinking / editing / stuck / idle (from hooks + file watching)
- Cross-session conflict detection: continuous `collect_file_overlaps()` via daemon
- Checkpoint engine: git SHA + task state + scrollback snapshots
- Restore: `kild restore` recreates sessions from last checkpoint

**Value**: Never lose work. Get alerted when agents need attention without watching every
pane. First multiplexer that understands what agents are doing.

### Phase 3: Session orchestration + Tōryō dashboard

**Goal**: Full session graph, forking, templates. kild-ui becomes the command center.

**Deliverables**:
- Session forking: `kild fork <branch> <checkpoint>` — try two approaches in parallel
- Session graph (DAG): define dependencies between sessions, auto-start downstream
- Templates: codify workflows as reusable TOML templates
- Tōryō dashboard mode in kild-ui: fleet overview, merge readiness, session graph
  visualization, cross-session intelligence summary
- Tool call rendering: syntax-highlighted cards inline with terminal output (GPUI-only feature)

**Value**: Sophisticated multi-agent workflows. The Tōryō view for fleet command.
No one else has session forking or interactive session graphs.

### Phase 4: Portability + remote access

**Goal**: Sessions move between machines. Remote monitoring.

**Deliverables**:
- Session serialization: export/import full session state
- IPC over TCP: daemon accessible from remote machines
- Remote kild-ui: connect to daemon on another machine
- Mobile client: read-only monitoring, approve/deny decisions
- Cloud deployment: daemon runs on VPS, agents run in containers

**Value**: Full #227 vision. Start locally, continue on cloud. Monitor from phone.

---

## Open Questions

### Technical

1. **tmux shim verification**: Which exact tmux commands does Claude Code use? Need to
   audit Claude Code's JavaScript source on npm.

2. **PTY multiplexing performance**: How many simultaneous PTYs can the daemon manage
   efficiently? What's the polling/epoll strategy?

3. **Checkpoint storage**: Where to store checkpoints? Size implications? Pruning strategy?

4. **GPUI + alacritty_terminal integration**: Zed's `TerminalElement` is the reference
   implementation. Key questions:
   - Can we extract/adapt Zed's terminal element, or must we write from scratch?
   - How to bridge `alacritty_terminal`'s `Term<T>` event listener with GPUI's model updates?
   - Performance: Is 4ms batching sufficient for KILD's use case (multiple terminals)?
   - Font rendering: Zed uses its own font system; kild-ui uses GPUI's built-in text system.
     Need to verify GPUI's `text_system().shape_line()` works for terminal cell rendering.

5. **GPUI version pinning**: Published crate is 0.2.2, but Zed uses unpublished main branch
   features. Need to decide: pin to published version or use git dependency? The terminal
   Element trait API may differ between published and main.

6. **Agent protocol**: Beyond hooks, how to get real-time agent state? Parse terminal output
   for known patterns (tool call headers, thinking indicators)? Or is hook-level granularity
   sufficient for Phase 1-2?

7. **IPC protocol**: Start with JSON over unix socket. When do we need something more
   structured (protobuf, gRPC)? Probably not until Phase 4 (remote access).

8. **Daemon ↔ kild-ui PTY streaming**: How to efficiently stream PTY bytes from daemon to
   kild-ui over IPC? Options: shared memory, unix socket with framing, file descriptor
   passing. Need to benchmark.

### Strategic

9. **Anthropic relationship**: When to approach about `teammateMode: "kild"`? After Phase 1
   ships and proves value.

10. **Other agent CLIs**: Kiro, Gemini CLI, Codex — do they have similar multiplexer
    integration? Need to research as each launches team features.

11. **Naming**: Is the daemon called `kildd`? `kild-daemon`? `kild serve`? Does the GPUI
    multiplexer replace the current kild-ui or evolve it?

12. **Workspace structure**: New crates needed: `kild-daemon`, `kild-tmux-shim`. The
    existing `kild-ui` evolves to become the GPUI multiplexer. Approximate crate layout:
    - `kild` — CLI (existing)
    - `kild-core` — Business logic (existing)
    - `kild-ui` — GPUI multiplexer (evolves from current dashboard)
    - `kild-daemon` — Background daemon with PTY management and IPC server (new)
    - `kild-tmux-shim` — tmux compatibility binary (new)
    - `kild-peek` — Visual inspection CLI (existing)
    - `kild-peek-core` — Visual inspection library (existing)

---

## References

### Issues
- [#227 — Vision: Pluggable isolation backends and session daemon architecture](https://github.com/Wirasm/kild/issues/227)
- [#223 — Merge queue](https://github.com/Wirasm/kild/issues/223) (session graph generalization)

### External — Claude Code / Anthropic
- [Introducing Claude Opus 4.6](https://www.anthropic.com/research/claude-opus-4-6) (Feb 5, 2026)
- [Claude Code Agent Teams docs](https://code.claude.com/docs/en/agent-teams)
- [Claude Code Hooks docs](https://code.claude.com/docs/en/hooks)
- [Claude Code Settings docs](https://code.claude.com/docs/en/settings)
- [Claude Code Subagents docs](https://code.claude.com/docs/en/sub-agents)

### External — GPUI Terminal Rendering
- [Zed terminal core](https://github.com/zed-industries/zed/tree/main/crates/terminal) — Terminal state (alacritty_terminal wrapper)
- [Zed terminal view](https://github.com/zed-industries/zed/tree/main/crates/terminal_view) — GPUI Element implementation
- [Zed terminal architecture (DeepWiki)](https://deepwiki.com/zed-industries/zed/9.1-terminal-core)
- [Zed: Leveraging Rust and GPU to render UIs at 120 FPS](https://zed.dev/blog/videogame)
- [gpui-terminal](https://github.com/zortax/gpui-terminal) — Community GPUI terminal component
- [zTerm](https://github.com/zerx-lab/zTerm) — Full GPUI terminal emulator
- [gpui-ghostty](https://github.com/Xuanwo/gpui-ghostty) — Ghostty VT + GPUI integration

### External — Crates & Libraries
- [`gpui-component`](https://github.com/longbridge/gpui-component) — Longbridge UI component library (10K+ stars, 60+ components)
- [`gpui-component` docs](https://longbridge.github.io/gpui-component/) — Component documentation and examples
- [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal) — Terminal emulation engine
- [`portable-pty`](https://crates.io/crates/portable-pty) — Cross-platform PTY management
- [`gpui`](https://crates.io/crates/gpui) — GPU-accelerated UI framework (published 0.2.2)
- [`parking_lot`](https://crates.io/crates/parking_lot) — FairMutex for terminal state
- [`adabraka-ui`](https://github.com/Augani/adabraka-ui) — Alternative GPUI component library (accessibility-first, evaluated but not chosen)

### Internal — Brand Research
- `.claude/PRPs/branding/brand-system.html` — Tallinn Night design system (colors, typography, components)
- `.claude/PRPs/branding/mockup-embedded.html` — Embedded terminal multiplexer mockup (the target UI)
- `.claude/PRPs/branding/BRAND.md` — Brand Bible v2 (voice, terminology, visual identity)
- `.claude/PRPs/branding/VISION.md` — Vision & Mission (product strategy, expansion path)
