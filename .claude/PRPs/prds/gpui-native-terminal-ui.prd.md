# GPUI Native Terminal UI for Shards

## Meta: How to Think About This PRD

**This document is written for an AI agent who will implement the code.** Read this section first to understand the philosophy.

### First Principles Thinking

We build from the ground up. Each phase adds ONE primitive capability. No phase should "assume" functionality from a later phase. If you find yourself thinking "I'll need X from Phase 6 to make Phase 3 work" - stop. Phase 3 should work standalone.

### Shards-First, Not Terminal-First

**Critical insight**: We're building a shard management dashboard that happens to show terminals, NOT a terminal app that happens to manage shards.

The core value is:
1. See all your shards in one place
2. Create/destroy/restart shards with clicks
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
- CLI: scripting, CI/CD, quick one-off shards, headless servers
- UI: visual management, dashboard, favorites

Both share the same core (sessions, git, config). Don't duplicate core logic in the UI.

---

## Problem Statement

Shards CLI works well but requires remembering commands and running them repeatedly. There's no visual dashboard to see all shards at once, check their status, or manage them with clicks. We need a GUI that provides visual shard management while reusing the CLI's proven terminal-launching code.

## Evidence

- Managing multiple shards requires repeated `shards list` / `shards status` commands
- No visual overview of all active shards
- Users must remember CLI syntax for create/destroy/restart
- No favorites system for frequently-used repositories

## Proposed Solution

Build a native GPUI application as a **visual dashboard** for shard management. For MVP, shard creation launches external terminals (exactly like CLI). The UI provides:

1. **Visual dashboard** - See all shards in a list with status
2. **Click-to-manage** - Create, destroy, restart with buttons
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
| Destroy button | Destroys shard (worktree cleanup, terminal close) |
| Restart button | Restarts agent in existing shard |
| Status indicators | Running/stopped, process health |
| Favorites | Quick-spawn into favorite repositories |

## What We're NOT Building (MVP)

| Feature | Why Not |
|---------|---------|
| Embedded terminals | Future enhancement, not MVP |
| Cross-platform (Linux/Windows) | macOS first, cross-platform comes with embedded terminals |
| Cross-shard orchestration | Future vision |
| Terminal multiplexing | Out of scope |
| Custom themes | Use defaults |

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

| # | Phase | Focus | Deliverable |
|---|-------|-------|-------------|
| 1 | Project Scaffolding | GPUI deps, feature gate | `cargo check --features ui` passes |
| 2 | Empty Window | GPUI opens a window | Window appears |
| 3 | Shard List View | Display existing shards | See shards from ~/.shards/sessions/ |
| 4 | Create Shard | Create button + dialog | Creates shard, launches external terminal |
| 5 | Destroy & Restart | Management buttons | Can destroy and restart shards |
| 6 | Status Dashboard | Health indicators, refresh | Live status updates |
| 7 | Favorites | Quick-spawn repos | Favorites work |

### Dependency Graph

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
   │         │         │          │          │          │         │
   │         │         │          │          │          │         └─ Convenience
   │         │         │          │          │          └─ Polish (live updates)
   │         │         │          │          └─ Full management (destroy, restart)
   │         │         │          └─ Core action (create shard)
   │         │         └─ Core view (see shards)
   │         └─ GPUI works
   └─ Build system works
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

### Phase 5: Destroy & Restart

**Goal**: Buttons to destroy and restart shards.

**Why this phase exists**: Complete the management loop - not just create, but also destroy and restart.

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

---

### Phase 6: Status Dashboard

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

### Phase 7: Favorites

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

## Future Phases (Post-MVP)

These come after the core dashboard is working and validated.

---

### Phase 8+: Embedded Terminals

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
| 8a | PTY infrastructure (spawn, read, write) - no UI |
| 8b | Basic terminal view (raw output in window) |
| 8c | Full terminal rendering (colors, cursor, input) |
| 8d | Replace external launch with embedded option |
| 8e | User preference: embedded vs external |

**Open questions for Phase 8**:
- Should embedded be the default, or opt-in?
- Should we support both embedded AND external (user choice)?
- What's the minimum viable terminal rendering? (Do we need full xterm compatibility?)

---

### Phase 9+: Cross-Shard Orchestration (Future Vision)

**Goal**: Enable a main session to coordinate child shards.

**Why this is the long-term vision**: The ultimate value of Shards is coordinating multiple AI agents working on different parts of a codebase. Today, humans manually switch between terminals and copy/paste context. With orchestration, a main agent could:
- Spawn worker shards for subtasks
- Monitor their progress by reading output
- Send them additional instructions
- Collect their results

**Prerequisites**:
- Embedded terminals (Phase 8) - required to read/write to shards
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

Note: We don't need `alacritty_terminal` or PTY crates for MVP. Those come with embedded terminals (Phase 8+).

### Platform Requirements

| Platform | Status |
|----------|--------|
| macOS | Supported (MVP) |
| Linux | Future (requires embedded terminals) |
| Windows | Future (requires embedded terminals) |

---

## Scope Boundary

**This PRD covers Phases 1-7 only** (the MVP dashboard with external terminals).

Phases 8+ (embedded terminals, orchestration) are documented above for vision context, but are **separate PRDs** to be written after MVP validation. Don't let future vision creep into MVP implementation.

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
| Embedded terminals | Deferred to Phase 8+ | Hardest part, not needed for core value |
| UI framework | GPUI | Native performance, Rust ecosystem |
| State management | Simple struct | YAGNI - no complex state libraries |

---

*Status: DRAFT - aligned with ideas.md vision*
*Philosophy: Shards-first, KISS, YAGNI*
*Platform: macOS first*
