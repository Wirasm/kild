# GPUI Native Terminal UI for Shards

## Problem Statement

Shards currently launches AI agents in external terminal windows via AppleScript (macOS only), immediately losing all process control. Developers cannot read agent output programmatically, send prompts to running sessions, detect task completion, or coordinate work across multiple shards. This fire-and-forget model prevents the core use case: a main orchestrating agent that coordinates child shards working on different parts of a codebase.

## Evidence

- Current implementation in `src/terminal/operations.rs` uses AppleScript to spawn external terminals
- No mechanism exists to read PTY output or send input after spawn
- Session tracking relies solely on PID, with no output capture
- macOS-only: Linux and Windows users cannot use the tool at all
- User workflow requires manual Alt-Tab between terminal windows

## Proposed Solution

Build a native GPUI application with embedded terminals powered by `alacritty_terminal` **as a separate frontend** to the existing CLI. The CLI continues to work exactly as it does today - launching agents in external terminals (iTerm, Ghostty, Terminal.app). The UI provides an alternative for users who want orchestration capabilities.

**Architecture Principle: Two Frontends, One Core**

```
┌─────────────────────────────────────────────────────────────────┐
│                    shards-core (library)                         │
│     sessions │ git │ process │ config │ errors │ cleanup        │
│                                                                  │
│  SHARED: Session persistence, config, git operations, etc.      │
└─────────────────────────────────────────────────────────────────┘
           │                                    │
    ┌──────┴──────┐                      ┌─────┴─────┐
    ▼             │                      │           ▼
┌─────────────────┴───┐              ┌───┴─────────────────┐
│   shards (CLI)      │              │   shards-ui         │
│                     │              │                     │
│ • Launch iTerm      │              │ • Embedded PTY      │
│ • Launch Ghostty    │              │ • Multi-shard tabs  │
│ • Launch Terminal   │              │ • Orchestration     │
│ • Fire-and-forget   │              │ • Full I/O control  │
│                     │              │                     │
│ Use case:           │              │ Use case:           │
│ Quick one-off shard │              │ Multi-agent coord   │
│ Scripting/CI        │              │ Visual monitoring   │
│ Headless servers    │              │ Main agent control  │
└─────────────────────┘              └─────────────────────┘
                    │                │
                    └───────┬────────┘
                            ▼
                SHARED: ~/.shards/sessions/*.json
```

**Key Points:**
- CLI is NOT being replaced - it keeps full external terminal support
- UI is an ADDITIONAL option for users who want orchestration
- Both share the same session state - a shard created in CLI appears in UI and vice versa
- Core library remains terminal-agnostic

**Why this approach:**
- GPUI is production-proven (powers Zed editor on all platforms)
- `alacritty_terminal` is the standard for embedded Rust terminals
- CLI users get zero bloat - UI dependencies are feature-gated
- Each frontend excels at its use case

## Key Hypothesis

We believe embedded PTY terminals with cross-shard communication will enable developers to orchestrate multiple AI agents from a single interface.
We'll know we're right when users can spawn 3+ shards and have a main agent read their outputs and send them commands without manual window switching.

## What We're NOT Building

- **Replacing CLI** - CLI remains fully functional with external terminal support (iTerm, Ghostty, etc.)
- **Removing external terminal support** - CLI keeps AppleScript/platform-native terminal launching
- **Remote terminals (SSH)** - Local execution only for v1
- **Terminal multiplexing (tmux-style splits)** - One terminal per shard
- **Custom themes** - Use GPUI/system defaults
- **Settings UI** - Config via TOML files
- **Plugin system** - No extensibility hooks
- **Session sync** - No cloud sync across machines

## Success Metrics

| Metric | Target | How Measured |
|--------|--------|--------------|
| Multi-shard orchestration | 3+ shards controlled from main session | Manual testing |
| Cross-platform | Works on macOS, Linux, Windows | CI builds + manual testing |
| CLI parity | All CLI-created sessions visible in UI | Integration test |
| Startup time | < 500ms to interactive | Benchmark |

## Open Questions

- [ ] Should we use gpui-component library for pre-built UI widgets or build custom?
- [ ] What's the right command syntax for cross-shard communication (`@shard:name` vs other)?
- [ ] Should output buffer be bounded (ring buffer) or unbounded with pagination?
- [ ] How do we detect when an AI agent is "idle" vs "thinking"?

---

## Users & Context

**Primary User**
- **Who**: Developer using multiple AI coding agents (Claude, Codex, etc.) on different branches/features
- **Current behavior**: Manually switches between terminal windows, copies output to share between agents
- **Trigger**: Starting work that spans multiple features or requires parallel agent coordination
- **Success state**: Single window showing all agents, with main agent orchestrating child shards

**Job to Be Done**
When I'm working on a complex feature that needs multiple parallel workstreams, I want to orchestrate AI agents from a single interface, so I can coordinate their work without context-switching between terminal windows.

**Non-Users**
- Users who only need one agent at a time (CLI is sufficient)
- Users who prefer tmux/terminal-native workflows
- Teams requiring shared/collaborative agent sessions

---

## CLI vs UI: When to Use Which

Both frontends are first-class citizens. Neither is "better" - they serve different use cases.

| Use Case | Recommended | Why |
|----------|-------------|-----|
| Quick one-off shard | **CLI** | `shards create feature-x` → iTerm opens, done |
| Scripting/automation | **CLI** | No GUI dependencies, scriptable |
| CI/CD pipelines | **CLI** | Headless, no display required |
| Headless servers | **CLI** | No GPU/display needed |
| Multi-agent orchestration | **UI** | Main session controls child shards |
| Visual monitoring | **UI** | See all shards in tabs, status indicators |
| Reading agent output | **UI** | Full PTY access, can query output |
| Sending prompts to running agents | **UI** | Direct PTY write access |

### Interoperability Examples

```bash
# CLI creates a shard (opens in iTerm)
shards create auth-fix --agent claude

# UI can see and monitor that shard (read-only for external terminals)
shards ui
# → auth-fix appears in session list with status "external"

# UI creates a shard (embedded PTY)
# → Click [+] in UI, creates "feature-x" shard

# CLI can see UI-created shards
shards list
# → Shows both auth-fix (external) and feature-x (embedded)

# CLI can destroy any shard
shards destroy feature-x
# → UI updates to reflect destruction
```

**Note**: For external terminal shards (CLI-created), the UI can show status but cannot read output or send commands. Full orchestration only works for embedded PTY shards created in the UI.

---

## Solution Detail

### Core Capabilities (MoSCoW)

| Priority | Capability | Rationale |
|----------|------------|-----------|
| Must | Embedded PTY terminals with full I/O control | Foundation for all features |
| Must | Multi-shard tab interface | Visual management of multiple agents |
| Must | Cross-platform support (macOS, Linux, Windows) | GPUI now supports all three |
| Must | Session persistence (survives app restart) | Parity with CLI |
| Should | Main session can read any shard's output | Core orchestration feature |
| Should | Main session can send commands to any shard | Core orchestration feature |
| Should | Visual status indicators (running/idle/stopped) | UX for monitoring |
| Could | Keyboard shortcuts for shard switching | Power user efficiency |
| Could | Output search/filtering | Finding relevant agent output |
| Won't | Terminal multiplexing (splits) | Out of scope, adds complexity |
| Won't | Custom themes | Use system/GPUI defaults |

### MVP Scope

**Phase 1-2 (PTY + Basic UI)**: Single terminal window that can spawn a shell, accept input, display output with ANSI colors. Validates the GPUI + alacritty_terminal integration works.

**Phase 3 (Multi-Shard)**: Tab bar for multiple shards, create/destroy via UI, session persistence integration.

**Phase 4 (Orchestration)**: Main session can read/write to child shards via `@shard:name` syntax.

### User Flow

```
1. User runs `shards ui`
2. Main session terminal appears
3. User clicks [+] or types command to create shard "auth-fix"
4. New tab appears with "auth-fix" shard running claude
5. User switches to main session tab
6. User types: @shard:auth-fix status
7. Main session displays recent output from auth-fix shard
8. User types: @shard:auth-fix "Now add tests for the login flow"
9. Command appears in auth-fix shard's input
10. User monitors progress, coordinates multiple shards
```

---

## Technical Approach

**Feasibility**: HIGH

GPUI + alacritty_terminal is a proven combination (Zed editor uses this exact stack). The main complexity is threading: PTY I/O is blocking and must run on background threads with event marshaling to the UI thread.

**Architecture: Two Frontends, One Core**

```
┌─────────────────────────────────────────────────────────────────┐
│                    shards-core (library crate)                   │
│                                                                  │
│  ┌──────────┐ ┌─────┐ ┌─────────┐ ┌────────┐ ┌────────┐        │
│  │ sessions │ │ git │ │ process │ │ config │ │ errors │        │
│  └──────────┘ └─────┘ └─────────┘ └────────┘ └────────┘        │
│                                                                  │
│  • Session CRUD (create, list, destroy, restart)                │
│  • Git worktree management                                       │
│  • Process tracking with PID validation                          │
│  • Config hierarchy (defaults → user → project → CLI)           │
│  • NO terminal-specific code - frontends handle that            │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌─────────────────────────────┐   ┌─────────────────────────────┐
│   CLI Frontend              │   │   UI Frontend               │
│   `shards create/list/...`  │   │   `shards ui`               │
│                             │   │                             │
│  src/terminal/ (existing)   │   │  src/ui/ (new, feature-gated)│
│  • iTerm via AppleScript    │   │  src/pty/ (new)             │
│  • Ghostty via AppleScript  │   │  src/shard_manager/ (new)   │
│  • Terminal.app             │   │                             │
│  • Fire-and-forget spawn    │   │  • GPUI window              │
│                             │   │  • Embedded PTY terminals   │
│  UNCHANGED by this PRD      │   │  • Multi-shard tabs         │
│                             │   │  • Full I/O control         │
│                             │   │  • Cross-shard orchestration│
└─────────────────────────────┘   └─────────────────────────────┘
              │                               │
              └───────────────┬───────────────┘
                              ▼
                SHARED: ~/.shards/sessions/*.json
                (CLI-created shards visible in UI and vice versa)
```

**Threading Model** (following Zed's pattern)

```
┌──────────────────┐     ┌─────────────────────┐     ┌──────────────┐
│   UI Thread      │     │  PTY Event Loop     │     │   PTY I/O    │
│   (GPUI main)    │◄────│  (background task)  │◄────│  (blocking)  │
│                  │     │  batches @ 4ms      │     │              │
└──────────────────┘     └─────────────────────┘     └──────────────┘
        │                         ▲
        │ input events            │ output events
        └─────────────────────────┘
```

**Key Technical Decisions**

| Decision | Choice | Alternatives | Rationale |
|----------|--------|--------------|-----------|
| UI Framework | GPUI | egui, iced, tauri | Production-proven with Zed, native performance |
| Terminal emulation | alacritty_terminal | vte, termwiz | Battle-tested, Zed uses same approach |
| PTY threading | tokio spawn_blocking + channels | std::thread | Better integration with async ecosystem |
| State sync | Arc<FairMutex<Term>> | RwLock, std Mutex | parking_lot's FairMutex prevents starvation |
| Output buffer | Ring buffer (VecDeque) | Unbounded Vec | Memory bounded, configurable size |

**Technical Risks**

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| GPUI breaking changes (pre-1.0) | HIGH | Pin to specific version, prepare for updates |
| PTY I/O blocking UI | MEDIUM | Background thread + channel pattern from Zed |
| Windows PTY differences | MEDIUM | Use conpty, test early on Windows |
| Terminal rendering complexity | MEDIUM | Start with basic grid, iterate on ANSI support |

---

## Implementation Phases

### Phase Overview

| # | Phase | Description | Complexity | Risk | Depends |
|---|-------|-------------|------------|------|---------|
| 1 | PTY Foundation | PTY spawn/read/write with threading model | Medium | Medium | - |
| 2 | Basic GPUI Shell | Single terminal window with keyboard input | **High** | High | 1 |
| 3 | Multi-Shard Management | Tab bar, create/destroy, session persistence | Medium | Low | 2 |
| 4 | Cross-Shard Orchestration | @shard commands, output reading, streaming | Medium | Medium | 3 |

### Dependency Graph

```
Phase 1: PTY Foundation
    │
    │ provides: PtyHandle, spawn_pty(), channels for I/O
    │
    ▼
Phase 2: Basic GPUI Shell
    │
    │ provides: GPUI app, TerminalView, single working terminal
    │
    ▼
Phase 3: Multi-Shard Management
    │
    │ provides: ShardTabs, multiple terminals, session persistence
    │
    ▼
Phase 4: Cross-Shard Orchestration
    │
    │ provides: @shard commands, output reading, orchestration API
    │
    ▼
[MVP Complete]
```

### Phase-to-File Mapping

| Phase | New Files | Modified Files |
|-------|-----------|----------------|
| **1** | `src/pty/mod.rs`, `src/pty/types.rs`, `src/pty/handler.rs`, `src/pty/errors.rs`, `src/pty/event_loop.rs`, `src/shard_manager/mod.rs`, `src/shard_manager/types.rs`, `src/shard_manager/handler.rs`, `src/shard_manager/errors.rs` | `Cargo.toml`, `src/lib.rs` |
| **2** | `src/ui/mod.rs`, `src/ui/app.rs`, `src/ui/views/mod.rs`, `src/ui/views/main_view.rs`, `src/ui/views/terminal_view.rs` | `src/cli/app.rs`, `src/main.rs`, `src/core/config.rs` |
| **3** | `src/ui/views/shard_tabs.rs`, `src/ui/views/status_bar.rs` | `src/ui/views/main_view.rs`, `src/shard_manager/handler.rs` |
| **4** | `src/ui/commands.rs` | `src/shard_manager/handler.rs`, `src/ui/views/main_view.rs`, `src/ui/views/terminal_view.rs` |

---

### Phase 1: PTY Foundation

**Goal**: Establish PTY infrastructure with proper threading - NO UI yet

**Why First**: Everything else depends on being able to spawn and communicate with PTY processes. This phase is pure backend work that can be unit tested without any UI.

**Deliverables**:
| Deliverable | Description | Validation |
|-------------|-------------|------------|
| `src/pty/types.rs` | PtyHandle, PtySize, PtyStatus, OutputBuffer | `cargo check --features ui` |
| `src/pty/handler.rs` | spawn_pty(), write_to_pty(), read_from_pty(), resize_pty() | Unit tests |
| `src/pty/errors.rs` | PtyError enum with ShardsError impl | `cargo check` |
| `src/pty/event_loop.rs` | Background thread with channel-based event marshaling | Integration test |
| `src/shard_manager/types.rs` | ManagedShard, ShardStatus | `cargo check` |
| `src/shard_manager/handler.rs` | ShardManager with create/destroy/list | Unit tests |
| `Cargo.toml` | Feature-gated deps (gpui, alacritty_terminal, tokio, parking_lot) | `cargo check` passes without `--features ui` |

**Success Criteria**:
```bash
# 1. Build passes
cargo check --features ui

# 2. Unit test passes
cargo test --features ui pty::
cargo test --features ui shard_manager::

# 3. Integration test: spawn PTY, write, read back
# (test spawns shell, writes "echo hello", reads "hello" from output channel)
```

**Risks**:
| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| PTY platform differences | Medium | High | Test on macOS + Linux early, use portable_pty crate if needed |
| Blocking I/O stalls | Medium | High | Background thread pattern from Zed |
| Channel backpressure | Low | Medium | Use unbounded channels initially, add bounds later if needed |

---

### Phase 2: Basic GPUI Shell

**Goal**: Functional single-terminal window - the hardest phase

**Why Second**: Need a working terminal to iterate on. Once we have one terminal working, adding more (Phase 3) is incremental.

**Why High Complexity**:
- GPUI is pre-1.0 with sparse documentation
- Terminal rendering requires understanding alacritty_terminal's grid model
- Input handling must correctly forward to PTY
- ANSI escape sequence rendering is subtle

**Deliverables**:
| Deliverable | Description | Validation |
|-------------|-------------|------------|
| `src/ui/app.rs` | GPUI Application setup, window creation | Window opens |
| `src/ui/views/terminal_view.rs` | Terminal grid rendering with alacritty_terminal | See characters on screen |
| `src/ui/views/main_view.rs` | Main layout (just terminal for now) | Layout correct |
| `src/cli/app.rs` update | Add `ui` subcommand | `shards ui` recognized |
| Keyboard input | Forward keystrokes to PTY | Can type commands |
| ANSI colors | Render colored output | Colors display correctly |

**Success Criteria**:
```bash
# 1. Command works
cargo run --features ui -- ui
# → Window opens

# 2. Can interact
# Type: echo "hello world"
# See: hello world (in terminal)

# 3. Colors work
# Type: ls --color
# See: colored output

# 4. Resize works
# Drag window corner
# Terminal adjusts grid size
```

**Risks**:
| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| GPUI API breaks | High | High | Pin to exact version, prepare for updates |
| Terminal rendering bugs | High | Medium | Start simple (monochrome), add features incrementally |
| Input handling edge cases | Medium | Medium | Follow Zed's input handling patterns |

**Suggested Sub-phases** (for detailed planning):
- 2a: GPUI window opens with placeholder content
- 2b: Connect PTY, show raw output (no formatting)
- 2c: Proper terminal grid rendering
- 2d: Keyboard input handling
- 2e: ANSI color support

---

### Phase 3: Multi-Shard Management

**Goal**: Manage multiple shards via tab interface

**Why Third**: Single terminal is working, now we add the multi-shard capability that differentiates us from just running a terminal.

**Deliverables**:
| Deliverable | Description | Validation |
|-------------|-------------|------------|
| `src/ui/views/shard_tabs.rs` | Tab bar with shard names | Tabs render |
| `src/ui/views/status_bar.rs` | Shard count, active shard name | Status shows |
| Create shard UI | Button or Cmd+T to create new shard | Can create shard |
| Destroy shard UI | Close button on tab | Can close shard |
| Tab switching | Click tab or Cmd+1/2/3 | Can switch shards |
| Session persistence | Read/write ~/.shards/sessions/ | Shards survive restart |
| Status indicators | Visual dot (running/idle/stopped) | Status visible |

**Success Criteria**:
```bash
# 1. Create multiple shards
# Click [+] three times
# → Three tabs appear

# 2. Switch between them
# Click each tab
# → Correct terminal shows

# 3. Persistence
# Close app, reopen
# → Same three shards restored

# 4. CLI interop
shards list
# → Shows UI-created shards
```

**Risks**:
| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| State management complexity | Medium | Medium | Use GPUI's Model pattern for shared state |
| Session format changes | Low | Medium | Version the session JSON format |

---

### Phase 4: Cross-Shard Orchestration

**Goal**: Main session can control child shards - the value proposition

**Why Fourth**: Needs multiple shards to exist (Phase 3) before we can orchestrate them.

**Deliverables**:
| Deliverable | Description | Validation |
|-------------|-------------|------------|
| `src/ui/commands.rs` | Parse `@shard:name "command"` syntax | Unit tests |
| `send_to_shard()` | Write to specific shard's PTY | Command appears in shard |
| `read_shard_output()` | Get last N lines from shard | Output readable |
| `get_shard_status()` | Detect running/idle/stopped | Status accurate |
| Main session feedback | "Sent to shard: X" confirmation | User knows it worked |
| Output streaming | Optional: stream shard output to main | Real-time monitoring |

**Success Criteria**:
```bash
# 1. Send command to shard
# In main session, type:
@shard:auth-fix "echo hello from main"
# → See "hello from main" in auth-fix shard

# 2. Read shard output
@shard:auth-fix status
# → See recent output from auth-fix

# 3. Multiple shards
@shard:feature-x "npm test"
@shard:auth-fix "cargo build"
# → Both shards execute commands
```

**Risks**:
| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Idle detection inaccuracy | Medium | Medium | Start with simple timeout, iterate |
| Command syntax confusion | Low | Low | Clear documentation, error messages |

---

### Phase Validation Summary

| Phase | Entry Criteria | Exit Criteria |
|-------|---------------|---------------|
| 1 | - | PTY spawn/read/write works, all unit tests pass |
| 2 | Phase 1 complete | Window opens, can type and see output with colors |
| 3 | Phase 2 complete | 3+ shards in tabs, persistence works, CLI interop works |
| 4 | Phase 3 complete | Main session can send/read from child shards |

### Parallelism Notes

Phases must be **sequential** at the phase level - each depends on the previous.

**Within-phase parallelism** (for worktree-based development):
| Phase | Can Parallelize |
|-------|-----------------|
| 1 | `src/pty/` and `src/shard_manager/` can be developed in parallel after types.rs |
| 2 | After app.rs works: terminal_view.rs and input handling can parallel |
| 3 | Tab UI and session persistence can parallel |
| 4 | Command parsing and output streaming can parallel |

---

## Dependencies

### Required Crates (Feature-Gated)

```toml
[features]
default = []
ui = [
  "dep:gpui",
  "dep:alacritty_terminal",
  "dep:parking_lot",
  "dep:tokio"
]

[dependencies]
# Existing deps unchanged...

# UI-only dependencies (optional)
gpui = { version = "0.2", optional = true }
alacritty_terminal = { version = "0.25", optional = true }
parking_lot = { version = "0.12", optional = true }
tokio = { version = "1.49", features = ["full", "sync"], optional = true }

# Optional: Pre-built UI components
gpui-component = { version = "0.5", optional = true }
```

### Platform Requirements

| Platform | Graphics Backend | PTY Implementation | Status |
|----------|-----------------|-------------------|--------|
| macOS | Metal | native PTY | Supported |
| Linux | Vulkan (Blade) | native PTY | Supported |
| Windows | DirectX 11 | conpty | Supported (GPUI 0.2+) |

### Build Commands

```bash
# CLI only (default, minimal)
cargo build

# With UI (adds ~50MB, requires platform GPU libs)
cargo build --features ui

# Run UI
cargo run --features ui -- ui
```

---

## Files to Create/Modify

**New Files (all feature-gated behind `#[cfg(feature = "ui")]`):**

| File | Purpose |
|------|---------|
| `src/pty/mod.rs` | PTY module root |
| `src/pty/types.rs` | PtyHandle, OutputBuffer, PtySize, PtyStatus |
| `src/pty/handler.rs` | spawn_pty, write_to_pty, read_from_pty, resize_pty |
| `src/pty/errors.rs` | PtyError enum |
| `src/pty/event_loop.rs` | Background PTY event loop with batching |
| `src/shard_manager/mod.rs` | Shard manager module root |
| `src/shard_manager/types.rs` | ManagedShard, ShardStatus |
| `src/shard_manager/handler.rs` | ShardManager struct with CRUD + orchestration |
| `src/shard_manager/errors.rs` | ShardError enum |
| `src/ui/mod.rs` | UI module root |
| `src/ui/app.rs` | GPUI Application setup |
| `src/ui/views/mod.rs` | Views module root |
| `src/ui/views/main_view.rs` | Main application layout |
| `src/ui/views/terminal_view.rs` | Single terminal rendering |
| `src/ui/views/shard_tabs.rs` | Tab bar component |
| `src/ui/views/status_bar.rs` | Bottom status bar |
| `src/ui/commands.rs` | @shard command parsing |

**Modified Files:**

| File | Change |
|------|--------|
| `Cargo.toml` | Add optional dependencies and `ui` feature |
| `src/lib.rs` | Conditionally export ui, pty, shard_manager modules |
| `src/cli/app.rs` | Add `ui` subcommand (feature-gated) |
| `src/main.rs` | Handle `ui` command when feature enabled |
| `src/core/config.rs` | Add UiConfig struct (feature-gated) |

---

## Validation Commands

### Level 1: Static Analysis
```bash
cargo check && cargo check --features ui && cargo clippy --features ui
```

### Level 2: Unit Tests
```bash
cargo test --lib --features ui
```

### Level 3: Build
```bash
cargo build --features ui
```

### Level 4: Smoke Test
```bash
cargo run --features ui -- ui
# Window should open with terminal
```

### Level 5: PTY Validation
```bash
# In UI terminal:
echo "hello world"
# Should see "hello world" output
```

### Level 6: Multi-Shard Validation
```bash
# Create 3 shards via UI
# Switch between them
# Restart app
# Shards should persist
```

### Level 7: Orchestration Validation
```bash
# In main session:
@shard:test "echo orchestrated"
# Should see command sent to test shard
@shard:test status
# Should see recent output from test shard
```

---

## Research Summary

**Market Context**
- Zed editor proves GPUI + alacritty_terminal is production-ready
- No existing tools combine AI agent orchestration with embedded terminals
- Current AI coding tools (Cursor, Continue) don't support multi-agent workflows

**Technical Context**
- GPUI 0.2.2 released October 2025 with Windows support (DirectX 11)
- alacritty_terminal 0.25.1 is current stable, requires Rust 1.85+
- Zed's terminal implementation provides proven architecture patterns
- Codebase already has strong session/process tracking infrastructure

**Key Sources**
- [GPUI Official Site](https://www.gpui.rs/)
- [GPUI crates.io](https://crates.io/crates/gpui) - v0.2.2
- [alacritty_terminal crates.io](https://crates.io/crates/alacritty_terminal) - v0.25.1
- [Zed Terminal Core Architecture](https://deepwiki.com/zed-industries/zed/9.1-terminal-core)
- [Zed Windows Release](https://www.neowin.net/news/windows-1011-users-have-a-reason-to-rejoice-as-microsofts-rival-gains-platform-support/)
- [gpui-component Library](https://longbridge.github.io/gpui-component/)

---

## Decisions Log

| Decision | Choice | Alternatives | Rationale |
|----------|--------|--------------|-----------|
| CLI/UI relationship | Two frontends, shared core | Replace CLI with UI | CLI serves different use cases (scripting, CI, quick shards) |
| CLI terminal support | Keep external terminals | Remove in favor of UI | Users want iTerm/Ghostty for one-off work |
| UI Framework | GPUI | egui, iced, tauri | Proven with Zed, native perf, cross-platform |
| Terminal lib | alacritty_terminal | vte, termwiz | Battle-tested, Zed uses same |
| Architecture | Feature-gated single binary | Separate crates | Simpler, CLI stays lightweight |
| Threading | tokio + channels | std::thread | Async ecosystem integration |
| Shard manager location | src/shard_manager/ (ui feature-gated) | src/ui/shard_manager/ | Can be tested independently |
| Windows backend | DirectX 11 (via GPUI) | Vulkan | GPUI's choice, better compatibility |
| Session interop | Both frontends read/write same JSON | Separate session stores | Users can mix CLI and UI workflows |

---

*Generated: 2026-01-21*
*Status: DRAFT - ready for review*
