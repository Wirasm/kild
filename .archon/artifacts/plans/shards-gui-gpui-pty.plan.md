# Feature: Shards GUI with GPUI and PTY Integration

## Summary

Building a native GUI application using GPUI (the same framework powering Zed editor) that provides a visual interface for managing Shards sessions. The GUI features a sidebar showing projects and their active worktrees/sessions, with integrated terminal views using PTY for direct process control. This replaces the current CLI-only interface with a modern, GPU-accelerated desktop application.

## User Story

As a developer using multiple AI agents
I want a visual dashboard to manage my Shards sessions
So that I can easily see all active projects, switch between terminals, and control agent processes without command-line complexity

## Problem Statement

The current Shards CLI requires users to remember session names and use terminal commands to manage multiple AI agent sessions. Users lose track of active sessions and struggle to switch between different agent terminals efficiently.

## Solution Statement

A GPUI-based desktop application with a sidebar for project/session navigation and integrated terminal panels using PTY for direct process control. The GUI maintains the existing session management logic while providing visual feedback and easier interaction patterns.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | NEW_CAPABILITY                                    |
| Complexity       | HIGH                                              |
| Systems Affected | sessions, terminal, git, core, new gui module    |
| Dependencies     | gpui, portable_pty, alacritty_terminal, tokio    |
| Estimated Tasks  | 12                                                |

---

## UX Design

### Before State
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           TERMINAL-ONLY INTERFACE                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   $ shards create feature-auth --agent claude                              │
│   ✓ Created session: feature-auth                                          │
│   ✓ Launched terminal with Claude                                          │
│                                                                             │
│   $ shards list                                                            │
│   Active Sessions:                                                          │
│   - feature-auth (claude) - /path/to/worktree                             │
│   - bug-fix-123 (kiro) - /path/to/worktree                                │
│                                                                             │
│   $ # User must remember session names                                      │
│   $ # No visual feedback on session status                                 │
│   $ # Must switch between multiple terminal windows                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

USER_FLOW: CLI commands → External terminal windows → Manual session tracking
PAIN_POINT: No visual overview, scattered terminals, memory-based navigation
DATA_FLOW: CLI → File system → External processes (no direct control)
```

### After State
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              SHARDS GUI                                     │
├─────────────────┬───────────────────────────────────────────────────────────┤
│   PROJECTS      │                 TERMINAL VIEW                             │
│                 │                                                           │
│ ▼ MyProject     │  ┌─────────────────────────────────────────────────────┐ │
│   ● feature-auth│  │ claude@feature-auth:~/worktree/feature-auth$        │ │
│     (claude)    │  │ > Working on authentication system...               │ │
│   ● bug-fix-123 │  │ > Created new login component                       │ │
│     (kiro)      │  │ > Running tests...                                  │ │
│   + New Session │  │                                                     │ │
│                 │  │                                                     │ │
│ ▼ OtherProject  │  │                                                     │ │
│   ● refactor-api│  │                                                     │ │
│     (gemini)    │  │                                                     │ │
│                 │  └─────────────────────────────────────────────────────┘ │
│                 │                                                           │
│                 │  [Resize Handle] [Process: Running ●] [Kill] [Restart]   │
└─────────────────┴───────────────────────────────────────────────────────────┘

USER_FLOW: Click session → View terminal → Direct process control → Visual feedback
VALUE_ADD: Visual session overview, integrated terminals, process monitoring
DATA_FLOW: GUI events → PTY processes → Real-time output capture → UI updates
```

### Interaction Changes
| Location        | Before                    | After                     | User_Action      | Impact                    |
| --------------- | ------------------------- | ------------------------- | ---------------- | ------------------------- |
| Session List    | `shards list` command     | Visual sidebar            | Click to view    | Instant visual overview   |
| Terminal Access | External terminal windows | Integrated terminal panel | Click session    | No window switching       |
| Process Control | No direct control         | Kill/Restart buttons     | Click controls   | Direct process management |
| Session Status  | Text-based status         | Visual indicators (●○)    | Visual feedback  | Real-time status updates  |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/sessions/handler.rs` | 8-85 | Session creation pattern to MIRROR exactly |
| P0 | `src/sessions/types.rs` | 4-18 | Session data structures to REUSE |
| P0 | `src/terminal/handler.rs` | 8-45 | Current terminal spawning to REPLACE with PTY |
| P1 | `src/core/logging.rs` | 3-12 | Logging setup to EXTEND for GUI events |
| P1 | `src/sessions/operations.rs` | 150-200 | File persistence patterns to MAINTAIN |
| P2 | `src/sessions/errors.rs` | 4-35 | Error handling patterns to FOLLOW |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI Docs](https://www.gpui.rs/) | Getting Started | Basic GPUI application setup |
| [portable_pty v0.8](https://docs.rs/portable-pty/) | PtySystem API | Cross-platform PTY creation |
| [Zed GPUI Blog](https://zed.dev/blog/gpui-ownership) | Ownership & Data Flow | GPUI state management patterns |
| [gpui-ghostty](https://github.com/Xuanwo/gpui-ghostty) | Terminal Integration | Reference implementation |

---

## Patterns to Mirror

**NAMING_CONVENTION:**
```rust
// SOURCE: src/sessions/handler.rs:8-20
// COPY THIS PATTERN:
pub fn create_session(request: CreateSessionRequest) -> Result<Session, SessionError> {
    info!(
        event = "session.create_started",
        branch = request.branch,
        agent = agent
    );
    // Handler orchestration pattern
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/sessions/errors.rs:4-15
// COPY THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' already exists")]
    AlreadyExists { name: String },
}

impl ShardsError for SessionError {
    fn error_code(&self) -> &'static str { "SESSION_ALREADY_EXISTS" }
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:15-20
// COPY THIS PATTERN:
info!(
    event = "session.create_started",
    branch = request.branch,
    agent = agent,
    command = agent_command
);
```

**SESSION_PERSISTENCE:**
```rust
// SOURCE: src/sessions/operations.rs:150-180
// COPY THIS PATTERN:
pub fn save_session_to_file(session: &Session, sessions_dir: &Path) -> Result<(), SessionError> {
    let session_file = sessions_dir.join(format!("{}.json", session.id.replace('/', "_")));
    let temp_file = session_file.with_extension("json.tmp");
    fs::write(&temp_file, session_json)?;
    fs::rename(&temp_file, &session_file)?; // Atomic operation
}
```

**TERMINAL_SPAWNING_REPLACEMENT:**
```rust
// SOURCE: src/terminal/handler.rs:8-25
// REPLACE THIS PATTERN WITH PTY:
pub fn spawn_terminal(working_directory: &Path, command: &str) -> Result<SpawnResult, TerminalError> {
    // Current: External terminal via AppleScript
    // New: PTY creation with direct process control
}
```

**VERTICAL_SLICE_STRUCTURE:**
```rust
// SOURCE: src/sessions/mod.rs:1-10
// COPY THIS PATTERN:
pub mod errors;
pub mod handler;
pub mod operations;
pub mod types;

pub use errors::*;
pub use handler::*;
pub use types::*;
```

---

## Files to Change

| File                                  | Action | Justification                                    |
| ------------------------------------- | ------ | ------------------------------------------------ |
| `Cargo.toml`                          | UPDATE | Add GPUI, portable_pty, tokio dependencies      |
| `src/lib.rs`                          | UPDATE | Export new gui module                            |
| `src/main.rs`                         | UPDATE | Add GUI mode flag and launcher                   |
| `src/gui/mod.rs`                      | CREATE | GUI module exports                               |
| `src/gui/app.rs`                      | CREATE | Main GPUI application struct                     |
| `src/gui/types.rs`                    | CREATE | GUI-specific types and state                     |
| `src/gui/errors.rs`                   | CREATE | GUI-specific errors                              |
| `src/gui/components/mod.rs`           | CREATE | UI components module                             |
| `src/gui/components/sidebar.rs`       | CREATE | Project/session sidebar component                |
| `src/gui/components/terminal_view.rs` | CREATE | Terminal display component with PTY integration  |
| `src/gui/pty/mod.rs`                  | CREATE | PTY management module                            |
| `src/gui/pty/manager.rs`              | CREATE | PTY process lifecycle management                 |
| `src/gui/pty/types.rs`                | CREATE | PTY-specific types                               |
| `src/gui/state.rs`                    | CREATE | Application state management                     |
| `src/gui/events.rs`                   | CREATE | GUI event system                                 |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Advanced terminal features** - No split panes, tabs, or complex terminal emulation (use basic PTY output display)
- **Session sharing/collaboration** - Single-user local sessions only
- **Custom themes/styling** - Use default GPUI styling initially
- **Plugin system** - No extensibility features in v1
- **Session templates** - No saved session configurations
- **Multi-repository support** - Single Git repository per project
- **Network/remote sessions** - Local processes only
- **Session recording/playback** - No session history features

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` (dependencies)

- **ACTION**: ADD GPUI and PTY dependencies
- **IMPLEMENT**: Add gpui, portable_pty, tokio, alacritty_terminal dependencies
- **MIRROR**: Existing dependency format in Cargo.toml:8-16
- **DEPENDENCIES**: 
  ```toml
  gpui = { git = "https://github.com/zed-industries/gpui", branch = "main" }
  portable_pty = "0.8"
  tokio = { version = "1.0", features = ["full"] }
  alacritty_terminal = "0.24"
  ```
- **GOTCHA**: GPUI is not on crates.io, must use git dependency
- **VALIDATE**: `cargo check` - dependencies resolve correctly

### Task 2: CREATE `src/gui/mod.rs`

- **ACTION**: CREATE GUI module structure
- **IMPLEMENT**: Module exports following vertical slice pattern
- **MIRROR**: `src/sessions/mod.rs:1-10` - same export structure
- **IMPORTS**: Export app, components, pty, state, events, types, errors
- **VALIDATE**: `cargo check` - module compiles

### Task 3: CREATE `src/gui/types.rs`

- **ACTION**: CREATE GUI-specific type definitions
- **IMPLEMENT**: AppState, SessionView, ProjectView, TerminalState types
- **MIRROR**: `src/sessions/types.rs:4-18` - struct patterns with serde
- **PATTERN**: Use serde for serialization, Clone + Debug derives
- **VALIDATE**: `cargo check` - types compile

### Task 4: CREATE `src/gui/errors.rs`

- **ACTION**: CREATE GUI-specific error types
- **IMPLEMENT**: GuiError enum with PTY, Rendering, State variants
- **MIRROR**: `src/sessions/errors.rs:4-35` - thiserror pattern exactly
- **PATTERN**: Implement ShardsError trait, use #[from] for conversions
- **VALIDATE**: `cargo check` - errors compile

### Task 5: CREATE `src/gui/state.rs`

- **ACTION**: CREATE application state management
- **IMPLEMENT**: AppState struct with sessions, projects, active_terminal
- **PATTERN**: Use Arc<Mutex<T>> for shared state, implement state updates
- **IMPORTS**: Use existing Session types from sessions module
- **VALIDATE**: `cargo check` - state management compiles

### Task 6: CREATE `src/gui/events.rs`

- **ACTION**: CREATE GUI event system
- **IMPLEMENT**: GuiEvent enum for SessionSelected, TerminalOutput, ProcessExit
- **MIRROR**: `src/core/events.rs:5-15` - event naming conventions
- **PATTERN**: Use structured logging for GUI events
- **VALIDATE**: `cargo check` - events compile

### Task 7: CREATE `src/gui/pty/types.rs`

- **ACTION**: CREATE PTY-specific types
- **IMPLEMENT**: PtyProcess, PtyConfig, ProcessStatus types
- **PATTERN**: Wrap portable_pty types with domain-specific abstractions
- **IMPORTS**: `use portable_pty::{PtyPair, Child, CommandBuilder}`
- **VALIDATE**: `cargo check` - PTY types compile

### Task 8: CREATE `src/gui/pty/manager.rs`

- **ACTION**: CREATE PTY process lifecycle management
- **IMPLEMENT**: PtyManager with spawn_process, kill_process, read_output methods
- **REPLACE**: `src/terminal/handler.rs:8-45` - replace external terminal with PTY
- **PATTERN**: Use tokio for async I/O, channels for output streaming
- **GOTCHA**: Handle process cleanup properly to avoid zombies
- **VALIDATE**: `cargo check` - PTY manager compiles

### Task 9: CREATE `src/gui/pty/mod.rs`

- **ACTION**: CREATE PTY module exports
- **IMPLEMENT**: Export manager, types from PTY module
- **MIRROR**: `src/sessions/mod.rs:1-10` - same export pattern
- **VALIDATE**: `cargo check` - PTY module exports correctly

### Task 10: CREATE `src/gui/components/sidebar.rs`

- **ACTION**: CREATE sidebar component for project/session navigation
- **IMPLEMENT**: Sidebar struct implementing GPUI Render trait
- **PATTERN**: Use GPUI div, list, button elements for layout
- **REFERENCE**: [GPUI component examples](https://www.gpui.rs/) for Render implementation
- **VALIDATE**: `cargo check` - sidebar component compiles

### Task 11: CREATE `src/gui/components/terminal_view.rs`

- **ACTION**: CREATE terminal display component with PTY integration
- **IMPLEMENT**: TerminalView struct with PTY output rendering
- **PATTERN**: Use GPUI text rendering, handle terminal escape sequences
- **REFERENCE**: [gpui-ghostty terminal integration](https://github.com/Xuanwo/gpui-ghostty)
- **GOTCHA**: Handle terminal resizing and escape sequence parsing
- **VALIDATE**: `cargo check` - terminal view compiles

### Task 12: CREATE `src/gui/components/mod.rs`

- **ACTION**: CREATE components module exports
- **IMPLEMENT**: Export sidebar, terminal_view components
- **MIRROR**: Module export pattern from other modules
- **VALIDATE**: `cargo check` - components module exports correctly

### Task 13: CREATE `src/gui/app.rs`

- **ACTION**: CREATE main GPUI application struct
- **IMPLEMENT**: ShardsApp struct implementing GPUI App trait
- **PATTERN**: Initialize state, handle events, render UI layout
- **REFERENCE**: [GPUI app examples](https://zed.dev/blog/gpui-ownership) for App trait
- **INTEGRATE**: Connect sidebar, terminal_view, and PTY manager
- **VALIDATE**: `cargo check` - main app compiles

### Task 14: UPDATE `src/lib.rs`

- **ACTION**: ADD GUI module export
- **IMPLEMENT**: Add `pub mod gui;` to library exports
- **MIRROR**: Existing module exports in lib.rs
- **VALIDATE**: `cargo check` - library exports GUI module

### Task 15: UPDATE `src/main.rs`

- **ACTION**: ADD GUI mode flag and launcher
- **IMPLEMENT**: Add --gui flag to CLI, launch GPUI app when specified
- **PATTERN**: Use clap for CLI flag, conditional app launching
- **INTEGRATE**: Import and launch ShardsApp when --gui flag is used
- **VALIDATE**: `cargo run -- --gui` - GUI application launches

---

## Testing Strategy

### Unit Tests to Write

| Test File                                      | Test Cases                           | Validates           |
| ---------------------------------------------- | ------------------------------------ | ------------------- |
| `src/gui/tests/state.test.rs`                 | state updates, session management   | State management    |
| `src/gui/tests/events.test.rs`                | event creation, logging              | Event system        |
| `src/gui/pty/tests/manager.test.rs`           | PTY spawn, kill, output capture      | PTY lifecycle       |
| `src/gui/components/tests/sidebar.test.rs`    | session list rendering, click events | Sidebar component   |
| `src/gui/components/tests/terminal_view.test.rs` | terminal output display, resizing    | Terminal component  |

### Edge Cases Checklist

- [ ] PTY process crashes or exits unexpectedly
- [ ] Terminal output with escape sequences and colors
- [ ] Window resizing affects terminal dimensions
- [ ] Multiple sessions with same name (should error)
- [ ] Session files corrupted or missing
- [ ] GUI launched without Git repository
- [ ] PTY spawn fails due to system limits
- [ ] Terminal output buffer overflow handling

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test src/gui/
```

**EXPECT**: All GUI tests pass, coverage >= 70%

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, release build succeeds

### Level 4: GUI_VALIDATION

```bash
cargo run -- --gui
```

**EXPECT**: GUI launches, sidebar shows projects, terminal view renders

### Level 5: INTEGRATION_VALIDATION

Manual testing checklist:
- [ ] Create new session via GUI
- [ ] Session appears in sidebar
- [ ] Click session shows terminal output
- [ ] PTY process responds to input
- [ ] Kill button terminates process
- [ ] Session state persists between GUI restarts

### Level 6: CROSS_PLATFORM_VALIDATION

Test on multiple platforms:
- [ ] macOS: GUI launches and PTY works
- [ ] Linux: GUI launches and PTY works  
- [ ] Windows: GUI launches and PTY works (if supported)

---

## Acceptance Criteria

- [ ] GUI application launches with `cargo run -- --gui`
- [ ] Sidebar displays all active sessions from existing file storage
- [ ] Clicking a session shows its terminal output in real-time
- [ ] PTY integration allows direct input/output to agent processes
- [ ] Process control buttons (kill/restart) work correctly
- [ ] Session creation through GUI creates proper worktrees and PTY processes
- [ ] All existing CLI functionality remains unchanged
- [ ] Level 1-3 validation commands pass with exit 0
- [ ] GUI integrates with existing session persistence (JSON files)
- [ ] Terminal view handles colors, escape sequences, and resizing

---

## Completion Checklist

- [ ] All 15 tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: `cargo check && cargo clippy` passes
- [ ] Level 2: `cargo test src/gui/` passes
- [ ] Level 3: `cargo test && cargo build --release` succeeds
- [ ] Level 4: GUI launches and renders correctly
- [ ] Level 5: Manual integration testing passes
- [ ] All acceptance criteria met
- [ ] Existing CLI functionality unaffected

---

## Risks and Mitigations

| Risk                           | Likelihood | Impact | Mitigation                                      |
| ------------------------------ | ---------- | ------ | ----------------------------------------------- |
| GPUI API instability          | HIGH       | HIGH   | Pin to specific git commit, test frequently     |
| PTY cross-platform issues     | MEDIUM     | HIGH   | Use portable_pty, test on all target platforms |
| Terminal escape sequence bugs  | MEDIUM     | MEDIUM | Start with basic text, add features gradually   |
| Performance with many sessions | LOW        | MEDIUM | Implement lazy loading, limit concurrent PTYs   |
| State synchronization issues   | MEDIUM     | HIGH   | Use Arc<Mutex<T>>, careful lock ordering        |

---

## Notes

**Design Decisions:**
- Using GPUI for native performance and GPU acceleration
- portable_pty for cross-platform PTY support instead of platform-specific solutions
- Maintaining existing file-based session persistence for compatibility
- Separating PTY management into its own module for testability

**Trade-offs:**
- GPUI git dependency adds build complexity but provides cutting-edge UI framework
- PTY integration adds system-level complexity but enables direct process control
- File-based persistence is simple but may not scale to hundreds of sessions

**Future Considerations:**
- Session templates and saved configurations
- Advanced terminal features (split panes, tabs)
- Plugin system for custom agents
- Session sharing and collaboration features
- Migration to SQLite for better session management
