# GPUI Native Terminal UI for Shards

## Meta: How to Think About This PRD

**This document is written for an AI agent who will implement the code.** Read this section first to understand the philosophy.

### First Principles Thinking

We build from the ground up. Each phase adds ONE primitive capability. No phase should "assume" functionality from a later phase. If you find yourself thinking "I'll need X from Phase 6 to make Phase 3 work" - stop. Phase 3 should work standalone.

### Shards-First, Not Terminal-First

**Critical insight**: We're building a shard management dashboard that happens to show terminals, NOT a terminal app that happens to manage shards.

The core value is:
1. See all your shards in one place
2. Create/open/stop/destroy shards with clicks
3. Track status and health

Embedded terminals are a **future enhancement**, not the MVP. For MVP, we launch external terminals (iTerm, Ghostty) exactly like the CLI does today. This:
- Delivers value faster
- Reuses existing, working code
- Defers the hardest technical challenge (terminal rendering)

### KISS and YAGNI

- **Keep It Simple, Stupid**: The simplest solution that works is the right solution
- **You Aren't Gonna Need It**: Don't build for hypothetical future needs

If you're writing code and think "this could be useful later" - delete it. Only write code that's needed for the current phase's validation criteria.

### macOS First

We're building for macOS first. The CLI already has working AppleScript integration for iTerm/Ghostty/Terminal.app. The UI will reuse this. Cross-platform support (Linux/Windows) comes later with embedded terminals.

### Why Feature-Gated?

The UI adds dependencies (GPUI, graphics backends). CLI users shouldn't pay this cost. The `--features ui` flag keeps the CLI lean. Never add UI dependencies outside the feature gate.

### Why Two Frontends?

CLI and UI serve different use cases. Neither replaces the other:
- CLI: scripting, CI/CD, quick one-off shards, headless servers, **agent orchestration**
- UI: visual management, dashboard, favorites

Both share the same core (sessions, git, config). Don't duplicate core logic in the UI.

### Target Users

See **[Personas Document](../personas.md)** for detailed user profiles:

1. **Power Users (Human)**: Agentic-forward engineers who want speed, control, and isolation. No hand-holding.
2. **Agents (AI)**: AI agents running inside shards that use the CLI to orchestrate work programmatically.

The CLI serves both personas. The UI serves only humans. Design CLI commands to work without TTY (agents can't respond to prompts).

---

## Problem Statement

Shards CLI works well but requires remembering commands and running them repeatedly. There's no visual dashboard to see all shards at once, check their status, or manage them with clicks. We need a GUI that provides visual shard management while reusing the CLI's proven terminal-launching code.

## Evidence

- Managing multiple shards requires repeated `shards list` / `shards status` commands
- No visual overview of all active shards
- Users must remember CLI syntax for create/open/stop/destroy
- No favorites system for frequently-used repositories

## Proposed Solution

Build a native GPUI application as a **visual dashboard** for shard management. For MVP, shard creation launches external terminals (exactly like CLI). The UI provides:

1. **Visual dashboard** - See all shards in a list with status
2. **Click-to-manage** - Create, open, stop, destroy with buttons
3. **Status tracking** - Running/stopped, process health, last activity
4. **Favorites** - Quick access to frequently-used repositories

**Architecture: Two Frontends, One Core**

```
┌─────────────────────────────────────────────────────────────────┐
│                    shards-core (library)                         │
│     sessions │ git │ process │ config │ errors │ cleanup        │
│                                                                  │
│  THIS CODE ALREADY EXISTS. UI reuses it, doesn't duplicate it.  │
└─────────────────────────────────────────────────────────────────┘
           │                                    │
    ┌──────┴──────┐                      ┌─────┴─────┐
    ▼             │                      │           ▼
┌─────────────────┴───┐              ┌───┴─────────────────┐
│   shards (CLI)      │              │   shards-ui         │
│                     │              │                     │
│ • Launch iTerm      │              │ • Visual dashboard  │
│ • Launch Ghostty    │              │ • Create/destroy UI │
│ • Fire-and-forget   │              │ • Status tracking   │
│                     │              │ • Favorites         │
│                     │              │                     │
│                     │              │ REUSES terminal     │
│                     │              │ launching from CLI  │
└─────────────────────┘              └─────────────────────┘
                    │                │
                    └───────┬────────┘
                            ▼
                SHARED: ~/.shards/sessions/*.json
```

---

## What We're Building (MVP)

| Feature | Description |
|---------|-------------|
| Shard list view | See all shards with names, status, branch |
| Create button | Opens dialog, creates shard → launches external terminal |
| Open button | Launch new agent in existing shard (additive) |
| Stop button | Close agent terminal(s), keep shard intact |
| Destroy button | Destroys shard (worktree cleanup) |
| Status indicators | Running/stopped, process health |
| Favorites | Quick-spawn into favorite repositories |

## What We're NOT Building (MVP)

| Feature | Why Not |
|---------|---------|
| Embedded terminals | Future enhancement, not MVP |
| Cross-platform (Linux/Windows) | macOS first, cross-platform comes with embedded terminals |
| Cross-shard orchestration | Future vision |
| Terminal multiplexing | Out of scope |
| User-selectable themes | Single polished default theme first |

## Success Metrics

| Metric | Target | How Measured |
|--------|--------|--------------|
| Feature parity | All CLI operations available in UI | Manual testing |
| Startup time | < 500ms to interactive | Benchmark |
| Session display | All shards visible with correct status | Manual testing |

---

## Implementation Phases

**Philosophy**: Each phase is ONE PR. Each phase has ONE focus. Each phase is testable in isolation.

### Phase Overview

| # | Phase | Focus | Deliverable | Status |
|---|-------|-------|-------------|--------|
| 1 | Project Scaffolding | GPUI deps, feature gate | `cargo check --features ui` passes | ✅ DONE |
| 2 | Empty Window | GPUI opens a window | Window appears | ✅ DONE |
| 3 | Shard List View | Display existing shards | See shards from ~/.shards/sessions/ | ✅ DONE |
| 4 | Create Shard | Create button + dialog | Creates shard, launches external terminal | ✅ DONE |
| 5 | Destroy & Restart | Management buttons | Can destroy and restart shards (basic) | ✅ DONE |
| 6 | Shard Lifecycle | Open/Stop/Destroy commands | Clean lifecycle for humans and agents | ✅ DONE |
| 7 | Status Dashboard | Health indicators, refresh | Live status updates, auto-refresh | ✅ DONE |
| 7.5 | Notes & Git Status | Session notes, git dirty indicator | Notes in list/create, uncommitted indicator | ✅ DONE |
| 7.6 | Bulk Operations | Open All / Stop All buttons | Bulk lifecycle operations | ✅ DONE |
| 7.7 | Quick Actions | Per-row action buttons | Copy Path, Open Editor, Focus Terminal | TODO |
| 8 | Favorites | Quick-spawn repos | Favorites work | TODO |
| 9 | Theme & Components | Color palette + reusable UI components | Polished design, extracted TextInput/Button/Modal | TODO |
| 10 | Keyboard Shortcuts | Full keyboard control | Navigate and operate UI without mouse | TODO |

### Dependency Graph

```
GUI Phases:
Phase 1 → 2 → 3 → 4 → 5 → 6 → 7 → 7.5 → 7.6 → 7.7 → 8 → 9 → 10
  ✅     ✅   ✅   ✅   ✅   ✅   │     │      │      │    │    │    │
                                │     │      │      │    │    │    └─ Keyboard control
                                │     │      │      │    │    └─ Theme polish
                                │     │      │      │    └─ Favorites
                                │     │      │      └─ Quick actions (copy, edit, focus)
                                │     │      └─ Bulk ops (open/stop all)
                                │     └─ Notes & git status
                                └─ Auto-refresh, status indicators

Cross-PRD Dependencies (CLI → GUI):
┌─────────────────────────────────────────────────────────────┐
│  CLI Phase 1.1 (--note)      ──────────►  GUI Phase 7.5     │
│  CLI Phase 1.2 (cd)          ──────────►  GUI Phase 7.7     │
│  CLI Phase 1.3 (code)        ──────────►  GUI Phase 7.7     │
│  CLI Phase 2.1 (focus)       ──────────►  GUI Phase 7.7     │
│  CLI Phase 2.5 (open/stop --all) ──────►  GUI Phase 7.6     │
└─────────────────────────────────────────────────────────────┘
```

---

### Phase 1: Project Scaffolding

**Goal**: Feature-gated dependencies compile. No functionality yet.

**Why this phase exists**: Validate that GPUI can be added as an optional dependency without breaking the existing CLI build.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/mod.rs` | `#[cfg(feature = "ui")] mod` declarations only |

**Files to Modify**:
| File | Change |
|------|--------|
| `Cargo.toml` | Add `ui` feature with optional deps |
| `src/lib.rs` | Conditionally export `ui` module |

**Cargo.toml additions**:
```toml
[features]
default = []
ui = ["dep:gpui"]

[dependencies]
gpui = { version = "0.2", optional = true }
```

**Validation**:
```bash
cargo check                    # CLI still works (MUST pass)
cargo check --features ui      # UI feature compiles (MUST pass)
cargo build                    # CLI binary size unchanged
```

**What NOT to do**:
- Don't write any implementation code
- Don't add dependencies outside the feature gate
- Don't modify any existing CLI code paths

---

### Phase 2: Empty Window

**Goal**: `shards ui` opens a GPUI window with placeholder text.

**Why this phase exists**: Validate GPUI works on this system. Window management, event loop, basic rendering.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/app.rs` | GPUI Application setup |
| `src/ui/views/mod.rs` | Views module |
| `src/ui/views/main_view.rs` | Shows "Shards" title text |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/cli/app.rs` | Add `ui` subcommand (feature-gated) |
| `src/main.rs` | Handle `ui` command |

**Validation**:
```bash
cargo run --features ui -- ui
# Window opens with "Shards" title
# Window can be resized
# Window can be closed (app exits cleanly)
```

**What NOT to do**:
- Don't load any shard data yet
- Don't add any buttons or interactions
- Don't connect to shards-core

---

### Phase 3: Shard List View

**Goal**: Display existing shards from `~/.shards/sessions/`.

**Why this phase exists**: The core value - seeing all your shards in one place. Read-only for now.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/views/shard_list.rs` | List component showing shards |
| `src/ui/state.rs` | UI state management (list of shards) |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/app.rs` | Load sessions on startup |
| `src/ui/views/main_view.rs` | Embed shard list |

**What to display per shard**:
- Shard name (branch name)
- Project name
- Status: "Running" or "Stopped" (based on process check)
- Agent type (claude, kiro, etc.)

**Validation**:
```bash
# First, create some shards via CLI
shards create test-shard-1
shards create test-shard-2

# Then open UI
cargo run --features ui -- ui
# See: list showing test-shard-1, test-shard-2
# See: correct status for each (Running if process exists)

# Destroy a shard via CLI
shards destroy test-shard-1

# Refresh/reopen UI
# See: only test-shard-2 remains
```

**What NOT to do**:
- Don't add create/destroy buttons yet
- Don't add click interactions
- Don't implement refresh button (manual reopen is fine)

---

### Phase 4: Create Shard

**Goal**: Create button that opens dialog, creates shard, launches external terminal.

**Why this phase exists**: The primary action - creating new shards from the UI.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/views/create_dialog.rs` | Modal/dialog for shard creation |
| `src/ui/actions.rs` | Action handlers (create, etc.) |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/views/main_view.rs` | Add [+] or "Create Shard" button |
| `src/ui/views/shard_list.rs` | Refresh after creation |

**Create dialog fields**:
- Branch name (required)
- Agent type (dropdown: claude, kiro, codex, custom)
- Base branch (optional, defaults to main)

**Key behavior**:
- Click "Create Shard" → dialog opens
- Fill in fields → click "Create"
- Calls existing `shards create` logic (shards-core)
- External terminal opens (iTerm/Ghostty/Terminal.app)
- Dialog closes, list refreshes, new shard appears

**Validation**:
```bash
cargo run --features ui -- ui
# Click "Create Shard" button
# Dialog opens
# Enter branch name: "test-from-ui"
# Select agent: claude
# Click "Create"
# External terminal opens with claude in new worktree
# Shard list shows "test-from-ui" as Running

# Verify via CLI
shards list
# Shows: test-from-ui
```

**What NOT to do**:
- Don't implement embedded terminal - use external
- Don't add destroy/restart yet
- Don't over-engineer the dialog (simple form is fine)

---

### Phase 5: Destroy & Restart (Basic)

**Goal**: Buttons to destroy and restart shards (basic implementation).

**Why this phase exists**: Complete the basic management loop - not just create, but also destroy and restart. This is a minimal implementation; Phase 6 adds proper lifecycle semantics and git safety.

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/views/shard_list.rs` | Add destroy [x] and restart [↻] buttons per shard |
| `src/ui/actions.rs` | Add destroy and restart handlers |

**Destroy behavior**:
- Click [x] on shard
- Confirmation: "Destroy shard 'name'? This removes the worktree."
- Calls existing `shards destroy` logic
- Shard removed from list

**Restart behavior**:
- Click [↻] on shard
- Calls existing `shards restart` logic
- Terminal window reactivated or new one opened
- Status updates to "Running"

**Validation**:
```bash
cargo run --features ui -- ui
# Create a shard (from Phase 4)

# Test destroy:
# Click [x] on shard
# Confirm dialog
# Shard disappears from list
shards list  # Confirms shard is gone

# Create another shard
# Close its terminal window manually
# Status shows "Stopped"

# Test restart:
# Click [↻]
# Terminal reopens
# Status shows "Running"
```

**What NOT to do**:
- Don't add bulk operations yet
- Don't add keyboard shortcuts yet
- Don't add git safety checks yet (that's Phase 6)

---

### Phase 6: Shard Lifecycle (Open/Stop/Destroy)

**Goal**: Proper Open/Stop/Destroy semantics. Applies to core, CLI, and UI.

**Why this phase exists**: Phase 5 added basic destroy/restart, but the UX is incomplete:
- `restart` is confusing - it closes existing terminal then opens new one (destructive)
- No way to add another agent to an existing shard
- No way to stop an agent without destroying the shard
- Agents can't use CLI to orchestrate (need non-interactive commands)

This phase adds clean lifecycle commands that work for both humans and agents.

---

#### Mental Model

```
┌─────────────────────────────────────────────────────────────────┐
│                          SHARD                                  │
│                  (Isolation Environment)                        │
│                                                                 │
│   Today: Git Worktree                                           │
│   Future: Docker, Seatbelt sandbox, VM, remote machine, etc.    │
│                                                                 │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐            │
│   │  Terminal   │  │  Terminal   │  │    IDE      │            │
│   │  + Agent    │  │  + Agent    │  │  (future)   │            │
│   │  (claude)   │  │  (kiro)     │  │             │            │
│   └─────────────┘  └─────────────┘  └─────────────┘            │
│                                                                 │
│   Multiple processes can run in the same shard                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Shard** = The isolation environment (git worktree)
- Persists until explicitly destroyed
- Can have multiple agents running in it

**Agent** = A terminal session running an AI agent
- Ephemeral - can be started/stopped without affecting the shard
- Multiple agents can run in the same shard simultaneously

**Key insight**: `open` is **additive** - it launches a new terminal without touching existing ones. This enables agent orchestration:

```bash
# Agent in main shard can spawn helpers:
shards open feature-auth --agent claude   # Spawn helper
shards open feature-auth --agent kiro     # Spawn another helper
shards stop feature-auth                   # Done, clean up
```

---

#### Three Actions

| Action | CLI Command | UI Button | What It Does |
|--------|-------------|-----------|--------------|
| **Open** | `shards open <branch>` | [▶] | Launch NEW agent terminal in shard (additive) |
| **Stop** | `shards stop <branch>` | [⏹] | Close agent terminal(s), keep shard intact |
| **Destroy** | `shards destroy <branch>` | [🗑] | Delete shard entirely |

**Key difference from `restart`**:
- `restart` = close existing + open new (destructive, confusing)
- `open` = just open new (additive, composable)

**Deprecate `restart`**: Mark as deprecated, internally implemented as `open`.

---

#### Git Error Handling

**Philosophy**: Trust git's natural guardrails. Don't add custom safety checks on top.

Git2 already refuses to remove worktrees with uncommitted changes. We surface these errors clearly:

```bash
shards destroy my-feature
# If git2 fails: "Cannot remove worktree: uncommitted changes in src/auth.rs"
# User knows exactly what to do: commit, stash, or --force
```

**CLI flags:**
```bash
shards destroy my-feature              # Normal destroy (git blocks if uncommitted)
shards destroy my-feature --force      # Force destroy (bypass git checks)
```

**No custom git status checks**. No warnings. No "are you sure?" prompts. Power users know what they're doing. Agents can't respond to prompts anyway.

---

#### Files to Modify

| File | Change |
|------|--------|
| `crates/shards-core/src/sessions/handler.rs` | Add `stop_session()`, `open_session()` |
| `crates/shards/src/commands/mod.rs` | Add `stop` and `open` commands, deprecate `restart` |
| `crates/shards/src/commands/destroy.rs` | Add `--force` flag |
| `crates/shards-ui/src/views/shard_list.rs` | Update buttons: [▶] Open / [⏹] Stop based on state |
| `crates/shards-ui/src/actions.rs` | Add `stop_shard()`, `open_shard()` |

---

#### Core Implementation

**New: `open_session(branch: &str, agent: Option<&str>)`**
```rust
// 1. Find session by branch
// 2. Verify worktree still exists
// 3. Spawn NEW terminal with agent (don't touch existing terminals)
// 4. Track new process in session
// 5. Update status to Running
```

**New: `stop_session(branch: &str)`**
```rust
// 1. Find session by branch
// 2. Kill tracked process(es)
// 3. Update session status to Stopped
// 4. Keep worktree intact
// 5. Keep session file
```

**Updated: `destroy_session(branch: &str, force: bool)`**
```rust
// 1. Find session by branch
// 2. Kill process (if running)
// 3. Remove worktree (git2 will block if uncommitted, unless force)
// 4. Remove session file
// 5. Surface any git errors clearly to user
```

---

#### CLI Commands

**New: `shards open`**
```bash
shards open <branch> [--agent <agent>]

# Opens NEW terminal in existing shard (additive)
shards open my-feature              # Open with default agent
shards open my-feature --agent kiro # Open with specific agent

# Can be called multiple times - each opens a new terminal
```

**New: `shards stop`**
```bash
shards stop <branch>

# Stops agent(s), keeps shard
shards stop my-feature
```

**Updated: `shards destroy`**
```bash
shards destroy <branch> [--force]

# Normal - git blocks if uncommitted changes
shards destroy my-feature

# Force - bypass git checks
shards destroy my-feature --force
```

**Deprecated: `shards restart`**
```bash
# Deprecated - use 'open' instead
shards restart my-feature
# Internally calls: open_session(branch, agent)
# Prints deprecation warning
```

---

#### UI Changes

**Shard list row buttons:**
```
Running:  ● feature-auth    claude    my-proj    [⏹] [🗑]
Stopped:  ○ fix-bug         kiro      my-proj    [▶] [🗑]
```

| State | Buttons |
|-------|---------|
| Running | [⏹] Stop, [🗑] Destroy |
| Stopped | [▶] Open, [🗑] Destroy |

**Destroy behavior**: Just destroy. If git blocks, show the error message. No custom warning dialogs.

---

#### Validation

**CLI validation:**
```bash
# Test open (additive behavior)
shards create test-open --agent claude
# Terminal 1 opens
shards open test-open --agent kiro
# Terminal 2 opens (Terminal 1 still running!)
shards list
# Shows: test-open (Running)

# Test stop
shards stop test-open
# Both terminals close
shards list
# Shows: test-open (Stopped)

# Test open after stop
shards open test-open
# New terminal opens
shards list
# Shows: test-open (Running)

# Test destroy
shards destroy test-open
shards list
# Shows: test-open gone

# Test git safety (natural guardrails)
shards create test-uncommitted --agent claude
echo "test" > ~/.shards/worktrees/*/test-uncommitted/test.txt
shards destroy test-uncommitted
# Should fail with clear git error message

shards destroy test-uncommitted --force
# Should succeed
```

**UI validation:**
```bash
cargo run -p shards-ui

# Test Open button on stopped shard
# Click [▶], terminal opens, status → Running

# Test Stop button on running shard
# Click [⏹], terminal closes, status → Stopped

# Test Destroy
# Click [🗑], shard removed (or error shown if git blocks)
```

**Agent orchestration validation:**
```bash
# Simulate agent spawning helpers
shards create main-task --agent claude
# In main-task terminal, agent runs:
shards create helper-1 --agent claude
shards open helper-1 --agent kiro  # Add second agent to same shard
# ... work happens ...
shards stop helper-1
shards destroy helper-1
```

---

**What NOT to do**:
- Don't add custom git safety checks (trust git2's natural behavior)
- Don't add confirmation prompts (power users + agents need speed)
- Don't implement "stop all" yet
- Don't add `--delete-branch` yet (YAGNI)
- Don't change the shard storage format

---

### Phase 7: Status Dashboard

**Goal**: Live status indicators and auto-refresh.

**Why this phase exists**: Polish - keep the dashboard current without manual refresh.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/refresh.rs` | Background refresh logic |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/views/shard_list.rs` | Add status indicators, timestamps |
| `src/ui/app.rs` | Add refresh timer |

**Status indicators**:
- 🟢 Running (process alive)
- 🔴 Stopped (process dead)
- ⚪ Unknown (can't determine)

**Additional info to show**:
- Last activity time (from session JSON or process)
- Created time
- Worktree path (on hover or expandable)

**Auto-refresh**:
- Poll every 5 seconds for process status
- Update indicators without full reload

**Validation**:
```bash
cargo run --features ui -- ui
# Create a shard
# See: 🟢 Running

# Close the terminal window
# Wait 5 seconds
# See: 🔴 Stopped (auto-updated)

# Restart the shard
# See: 🟢 Running (auto-updated)
```

**What NOT to do**:
- Don't add complex real-time streaming
- Don't add notifications
- Keep polling simple (5 second interval is fine)

---

### Phase 7.5: Notes & Git Status

**Goal**: Show session notes in list view, add note field to create dialog, show git dirty indicator.

**Why this phase exists**: Notes help users remember what each shard is for. Git status indicator shows at a glance which shards have uncommitted work.

**Dependencies**: Requires CLI `--note` feature (CLI Phase 1.1) to be implemented first.

**Files to Modify**:
| File | Change |
|------|--------|
| `crates/shards-ui/src/views/shard_list.rs` | Show note column, git dirty indicator |
| `crates/shards-ui/src/views/create_dialog.rs` | Add note text field |
| `crates/shards-ui/src/actions.rs` | Pass note to create_session |

**What to display per shard**:
- Existing: Branch, Agent, Status
- New: Note (truncated, full on hover)
- New: Git indicator (● if uncommitted changes)

**Create dialog additions**:
```
Branch: [____________]
Agent:  [claude ▼    ]
Note:   [____________]  ← NEW
        [  Create  ]
```

**Git status check**:
```rust
// In shard_list.rs, check for uncommitted changes
fn has_uncommitted_changes(worktree_path: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}
```

**Validation**:
```bash
# Create shard with note via CLI
shards create test-note --note "Working on auth feature"

# Open UI
cargo run -p shards-ui
# See: "Working on auth..." in list

# Create shard via UI with note
# Click Create, enter note, verify it appears in list

# Make changes in worktree
echo "test" >> ~/.shards/worktrees/*/test-note/test.txt
# See: ● indicator appears next to shard
```

**What NOT to do**:
- Don't show full git diff in list (that's Phase 7.7 quick actions)
- Don't block on git status check (async/cached)

---

### Phase 7.6: Bulk Operations

**Goal**: Add "Open All Stopped" and "Stop All Running" buttons to header.

**Why this phase exists**: Power users managing multiple shards need bulk operations. Enables quick "end of day" cleanup and "start of day" launch.

**Dependencies**: Requires CLI `open --all` and `stop --all` (CLI Phase 2.5) to be implemented first.

**Files to Modify**:
| File | Change |
|------|--------|
| `crates/shards-ui/src/views/main_view.rs` | Add bulk action buttons to header |
| `crates/shards-ui/src/actions.rs` | Add open_all_stopped(), stop_all_running() |
| `crates/shards-ui/src/state.rs` | Track bulk operation progress/errors |

**UI Layout**:
```
┌─────────────────────────────────────────────────────────┐
│  Shards                    [▶ Open All] [⏹ Stop All] [+]│
├─────────────────────────────────────────────────────────┤
│  ● feature-auth    claude    Running     Auth work...   │
│  ○ feature-api     kiro      Stopped     API refactor   │
│  ● bugfix-login    claude    Running                    │
└─────────────────────────────────────────────────────────┘
```

**Button states**:
| Button | Enabled When | Action |
|--------|--------------|--------|
| [▶ Open All] | Any shard is Stopped | Launch agents in all stopped shards |
| [⏹ Stop All] | Any shard is Running | Stop all running agents |

**Behavior**:
- Buttons disabled (grayed) when no applicable shards
- Shows count: "▶ Open All (2)" if 2 stopped
- Progress feedback during bulk operation
- Error summary if any fail

**Validation**:
```bash
cargo run -p shards-ui

# Create 3 shards, stop 2
# See: [▶ Open All (2)] enabled, [⏹ Stop All (1)] enabled

# Click "Open All"
# See: 2 terminals launch, button updates to (0), disabled

# Click "Stop All"
# See: All 3 stop, button updates to (0), disabled
```

**What NOT to do**:
- Don't add confirmation dialogs (power users)
- Don't add "Destroy All" (too dangerous for a button)

---

### Phase 7.7: Quick Actions

**Goal**: Per-shard action buttons for Copy Path, Open in Editor, Focus Terminal.

**Why this phase exists**: Quick access to common operations without leaving the UI or using CLI.

**Dependencies**:
- `shards cd` (CLI Phase 1.2) - for copy path logic
- `shards code` (CLI Phase 1.3) - for open in editor
- `shards focus` (CLI Phase 2.1) - for focus terminal

**Files to Modify**:
| File | Change |
|------|--------|
| `crates/shards-ui/src/views/shard_list.rs` | Add action buttons per row |
| `crates/shards-ui/src/actions.rs` | Add copy_path(), open_in_editor(), focus_terminal() |

**UI Layout** (row actions on hover or always visible):
```
│  ● feature-auth    claude    Running    [📋] [📝] [🎯] [⏹] [🗑] │
                                           │    │    │    │    │
                                           │    │    │    │    └─ Destroy
                                           │    │    │    └─ Stop
                                           │    │    └─ Focus Terminal
                                           │    └─ Open in Editor
                                           └─ Copy Path
```

**Actions**:
| Icon | Action | Behavior |
|------|--------|----------|
| 📋 | Copy Path | Copy worktree path to clipboard |
| 📝 | Open in Editor | Launch $EDITOR or VS Code with worktree |
| 🎯 | Focus Terminal | Bring shard's terminal window to front |

**Implementation**:
```rust
fn copy_path(session: &Session, cx: &mut Context) {
    cx.write_to_clipboard(session.worktree_path.display().to_string());
    // Show brief "Copied!" tooltip
}

fn open_in_editor(session: &Session) {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "code".into());
    Command::new(&editor)
        .arg(&session.worktree_path)
        .spawn()
        .ok();
}

fn focus_terminal(session: &Session) {
    if let Some(ref terminal_type) = session.terminal_type {
        if let Some(ref window_id) = session.terminal_window_id {
            terminal::focus_window(terminal_type, window_id);
        }
    }
}
```

**Validation**:
```bash
cargo run -p shards-ui

# Hover over a shard row
# See: Action buttons appear

# Click 📋 (Copy Path)
# Paste somewhere - verify correct path

# Click 📝 (Open in Editor)
# VS Code opens with worktree

# Click 🎯 (Focus Terminal)
# Terminal window comes to front
```

**What NOT to do**:
- Don't show git diff panel (future feature)
- Don't add too many buttons (keep it clean)

---

### Phase 8: Favorites

**Goal**: Store favorite repositories for quick shard creation.

**Why this phase exists**: Convenience - users work across multiple repos.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/favorites.rs` | Load/save favorites list |
| `src/ui/views/favorites_panel.rs` | Favorites sidebar or section |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/views/main_view.rs` | Add favorites panel |
| `src/ui/views/create_dialog.rs` | Pre-fill from favorite selection |

**Storage**: `~/.shards/favorites.json`
```json
{
  "favorites": [
    {"path": "/Users/x/projects/shards", "name": "shards"},
    {"path": "/Users/x/projects/other", "name": "other-project"}
  ]
}
```

**Key work**:
- Favorites panel showing saved repos
- "Add current directory to favorites" (if UI knows current dir)
- "Add from path" manual entry
- Click favorite → opens create dialog with path pre-filled
- Remove favorite button

**Validation**:
```bash
cargo run --features ui -- ui
# Add a favorite (manual path entry)
# See: favorite appears in list

# Click favorite
# Create dialog opens with path pre-filled

# Create shard from favorite
# Shard is created in that repo

# Close/reopen UI
# Favorites persist
```

---

### Phase 9: Theme & Components

**Goal**: Apply a polished color palette and extract reusable UI components.

**Why this phase exists**: After all functionality is complete, we polish the visual design and refactor the UI into reusable components. This phase transforms working-but-rough UI into a cohesive, maintainable design system.

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/theme.rs` | Color constants, theme struct |
| `src/ui/components/mod.rs` | Reusable component module |
| `src/ui/components/text_input.rs` | Extracted, polished text input |
| `src/ui/components/button.rs` | Styled button component |
| `src/ui/components/modal.rs` | Reusable modal/dialog wrapper |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/views/*.rs` | Replace hardcoded colors with theme constants |
| `src/ui/views/create_dialog.rs` | Use extracted components |
| `src/ui/views/main_view.rs` | Use theme and components |

**Color Palette**:

```
Primary Accent (Olive)
├── Olive Dark:   #5d6b50
├── Gray Olive:   #8a9a7a
└── Olive Bright: #a3b392

Secondary Accent (Peach)
├── Peach:        #fab387
└── Peach Dim:    #daa070

Functional
├── Success:      #8a9a7a (Gray Olive)
├── Warning:      #fab387 (Peach)
├── Error:        #f38ba8
└── Info:         #89b4fa

Dark Theme (Mocha)
├── Base:         #1e1e2e
├── Mantle:       #181825
├── Crust:        #11111b
├── Surface0:     #313244
├── Surface1:     #45475a
├── Text:         #cdd6f4
└── Subtext:      #bac2de

Terminal Chrome Dots
├── Red:          #f38ba8
├── Yellow:       #f9e2af
└── Green:        #8a9a7a
```

**Components to extract**:
- `TextInput` - Keyboard-captured text field with cursor, placeholder, validation state
- `Button` - Primary (olive), secondary (surface), danger (error) variants
- `Modal` - Overlay + centered dialog box with title, content, actions

**Validation**:
```bash
cargo run -p shards-ui
# Verify: All colors match the palette above
# Verify: Consistent styling across all views
# Verify: Components work in create dialog
# Verify: Theme applies to header, list, dialog, buttons
```

**What NOT to do**:
- Don't add light theme yet (dark only for MVP)
- Don't add theme switching UI
- Don't over-abstract (simple constants are fine)

---

### Phase 10: Keyboard Shortcuts

**Goal**: Full keyboard control of the UI - navigate, select, and operate without touching the mouse.

**Why this phase exists**: Power users (our target audience) live in the keyboard. A dashboard that requires mouse clicks for everything is friction. Think Vim, Neovim, Superhuman - every action should be reachable via keyboard.

**Philosophy**:
- **Vim-inspired** - Modal where it makes sense, mnemonic shortcuts
- **Discoverable** - Show available shortcuts contextually
- **Consistent** - Same patterns throughout the UI
- **Escapable** - Escape always returns to normal state

**Files to Create**:
| File | Purpose |
|------|---------|
| `src/ui/keybindings.rs` | Shortcut definitions and action mapping |
| `src/ui/views/shortcut_hint.rs` | Footer showing available shortcuts |

**Files to Modify**:
| File | Change |
|------|--------|
| `src/ui/views/main_view.rs` | Add global key handlers, selection state |
| `src/ui/views/shard_list.rs` | Add selection highlight, keyboard navigation |
| `src/ui/views/create_dialog.rs` | Tab navigation between fields |

**Core shortcuts** (tentative - refine during implementation):

| Context | Key | Action |
|---------|-----|--------|
| Global | `c` | Open create dialog |
| Global | `?` | Show all shortcuts |
| Global | `/` | Focus search (future) |
| Global | `r` | Refresh list |
| List | `j` / `↓` | Move selection down |
| List | `k` / `↑` | Move selection up |
| List | `Enter` | Open selected shard details (future) |
| List | `d` | Destroy selected shard (with confirm) |
| List | `s` | Restart selected shard |
| List | `g` | Go to first item |
| List | `G` | Go to last item |
| Dialog | `Tab` | Next field |
| Dialog | `Shift+Tab` | Previous field |
| Dialog | `Enter` | Submit |
| Dialog | `Escape` | Cancel/close |

**Selection model**:
- One shard selected at a time (highlighted row)
- Selection persists across refreshes (by session ID)
- No selection = first item selected on `j`/`k`

**Shortcut hints footer**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│  j/k Navigate  │  c Create  │  d Destroy  │  s Restart  │  ? Help      │
└─────────────────────────────────────────────────────────────────────────┘
```

**Validation**:
```bash
cargo run -p shards-ui
# Navigate list with j/k
# Create shard with 'c' key
# Fill dialog, submit with Enter
# Select shard, destroy with 'd'
# Press '?' to see all shortcuts
# Verify Escape closes any dialog/modal
```

**What NOT to do**:
- Don't implement command palette yet (future enhancement)
- Don't add custom keybinding configuration
- Don't conflict with system shortcuts (Cmd+Q, Cmd+W, etc.)
- Don't require modifier keys for common actions (keep it simple: just `j`, not `Ctrl+j`)

---

## Future Phases (Post-MVP)

These come after the core dashboard is working and validated.

---

### Phase 11+: Embedded Terminals

**Goal**: Replace external terminals with embedded PTY terminals in the UI.

**Why this matters**: Embedded terminals give us **control**. With external terminals (iTerm, Ghostty), we can only:
- Launch them (fire-and-forget)
- Check if the process is running
- Kill the process

With embedded terminals, we gain:
- Full read access to terminal output
- Full write access to send commands
- Knowledge of terminal state (cursor position, screen content)
- Cross-platform support (no AppleScript dependency)

**Why deferred to post-MVP**:
1. It's the hardest technical challenge (GPUI + alacritty_terminal + PTY threading)
2. The dashboard delivers core value without it
3. We want to validate the product concept before investing in the hard work
4. External terminals work fine for macOS users

**What it enables**:

| Capability | Why It Matters |
|------------|----------------|
| Cross-platform | Linux and Windows users can use Shards |
| Terminal tabs in UI | No window switching, all shards visible |
| Output reading | See what agents are doing without switching windows |
| Foundation for orchestration | Required for future @shard commands |

**Technical approach** (research needed):
- Use `alacritty_terminal` crate for terminal emulation (ANSI parsing, grid state)
- Use platform PTY APIs (or `portable_pty` crate) for process spawning
- Background thread for PTY I/O, channel-based communication to UI thread
- Reference: Zed editor uses this exact stack (GPUI + alacritty_terminal)

**Rough phase breakdown** (to be detailed in separate PRD):

| Sub-phase | Focus |
|-----------|-------|
| 11a | PTY infrastructure (spawn, read, write) - no UI |
| 11b | Basic terminal view (raw output in window) |
| 11c | Full terminal rendering (colors, cursor, input) |
| 11d | Replace external launch with embedded option |
| 11e | User preference: embedded vs external |

**Open questions for Phase 11**:
- Should embedded be the default, or opt-in?
- Should we support both embedded AND external (user choice)?
- What's the minimum viable terminal rendering? (Do we need full xterm compatibility?)

---

### Phase 12+: Cross-Shard Orchestration (Future Vision)

**Goal**: Enable a main session to coordinate child shards.

**Why this is the long-term vision**: The ultimate value of Shards is coordinating multiple AI agents working on different parts of a codebase. Today, humans manually switch between terminals and copy/paste context. With orchestration, a main agent could:
- Spawn worker shards for subtasks
- Monitor their progress by reading output
- Send them additional instructions
- Collect their results

**Prerequisites**:
- Embedded terminals (Phase 11) - required to read/write to shards
- Output buffering - store terminal history for querying
- Command protocol - how main session addresses child shards

**Possible features** (research needed):
- `@shard:name status` - get recent output from a shard
- `@shard:name "do something"` - send command to a shard
- `@shard:all status` - overview of all shards
- Idle detection - know when an agent is waiting for input

**Why deferred far into the future**:
1. Requires embedded terminals first
2. Needs research into feasibility and UX
3. Core dashboard value doesn't depend on it
4. May require agent SDK integration, not just terminal tricks

**This is speculative**. We note it here to preserve the vision, but it's not committed scope. The right time to design this is after embedded terminals are working and we understand the possibilities.

---

## Dependencies

### Required Crates (Feature-Gated)

```toml
[features]
default = []
ui = ["dep:gpui"]

[dependencies]
gpui = { version = "0.2", optional = true }
```

Note: We don't need `alacritty_terminal` or PTY crates for MVP. Those come with embedded terminals (Phase 11+).

### Platform Requirements

| Platform | Status |
|----------|--------|
| macOS | Supported (MVP) |
| Linux | Future (requires embedded terminals) |
| Windows | Future (requires embedded terminals) |

---

## Scope Boundary

**This PRD covers Phases 1-10** (the MVP dashboard with external terminals, lifecycle management, polish, and keyboard control).

Phases 11+ (embedded terminals, orchestration) are documented above for vision context, but are **separate PRDs** to be written after MVP validation. Don't let future vision creep into MVP implementation.

---

## Open Questions

- [ ] Should we use gpui-component library for pre-built UI widgets?
- [ ] How should the create dialog look? (modal vs inline)
- [ ] Should favorites be global or per-project?
- [ ] Should we show a "current directory" indicator for context?

---

## Guidance for Implementing Agent

### Before Starting Any Phase

1. Read the phase description completely
2. Read "What NOT to do" - these are common mistakes
3. Check the validation criteria - this is your definition of done
4. Don't look ahead to future phases

### The Core Pattern

Every phase follows this pattern:
1. Call existing shards-core functions (don't duplicate logic)
2. Update UI state
3. Re-render

Example for "Create Shard":
```
User clicks button → Show dialog → User fills form →
Call shards::create() → Update state.shards → Re-render list
```

### When You're Stuck

1. Check if you're over-engineering (YAGNI)
2. Check if you're building for a future phase
3. Look at how the CLI does it - then call that same code
4. Ask for clarification rather than guessing

### Code Quality

- All code must be feature-gated under `#[cfg(feature = "ui")]`
- All types must have proper type annotations
- Run `cargo check` and `cargo check --features ui` before committing
- Reuse shards-core logic - never duplicate

### Phase Boundaries

Each phase is a PR. Don't bleed work across phases. If Phase 3 validation passes, stop and submit. Don't "just add" the create button because it seems easy - that's Phase 4.

---

## Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Terminal approach | External terminals for MVP | Faster to value, reuses working code |
| Platform | macOS first | CLI terminal launching already works |
| Embedded terminals | Deferred to Phase 11+ | Hardest part, not needed for core value |
| UI framework | GPUI | Native performance, Rust ecosystem |
| State management | Simple struct | YAGNI - no complex state libraries |
| Git safety checks | Trust git2's natural guardrails | No custom checks; surface errors clearly |
| `restart` command | Deprecated in favor of `open` | `open` is additive, `restart` was destructive |
| Agent as CLI user | First-class persona | Agents orchestrate via CLI; no interactive prompts |

---

*Status: DRAFT - aligned with ideas.md vision*
*Philosophy: Shards-first, KISS, YAGNI*
*Platform: macOS first*
