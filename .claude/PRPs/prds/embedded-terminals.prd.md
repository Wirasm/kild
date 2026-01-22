# Embedded PTY Terminals for Shards UI

**Status**: VISION - Post-MVP enhancement
**Depends on**: MVP Dashboard (Phases 1-7)
**Enables**: Orchestrator Agent SDK, Cross-platform support

---

## Meta: Why This Document Exists

This PRD captures the vision and technical approach for embedded terminals. We're documenting this now because:

1. It explains WHY we're building the MVP with external terminals first
2. It's the bridge between "dashboard" and "orchestration"
3. It captures technical research (GPUI, alacritty_terminal, Zed patterns)
4. It enables cross-platform support (Linux, Windows)

**Build the MVP dashboard first.** This comes after.

---

## The Vision

Replace external terminal launching (iTerm, Ghostty, Terminal.app) with embedded PTY terminals inside the Shards UI itself.

### Before (MVP - External Terminals)

```
Shards UI                    External Terminal
┌─────────────┐              ┌─────────────┐
│             │   launches   │             │
│  Dashboard  │ ──────────►  │  iTerm      │
│             │   (fire &    │  + claude   │
│  [Create]   │   forget)    │             │
│  [Destroy]  │              │  (no ctrl)  │
└─────────────┘              └─────────────┘
```

### After (This PRD - Embedded Terminals)

```
Shards UI
┌─────────────────────────────────────────────┐
│                                             │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐       │
│  │ Shard 1 │ │ Shard 2 │ │ Shard 3 │  tabs │
│  └─────────┘ └─────────┘ └─────────┘       │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │ $ claude                            │   │
│  │ > I'll help you with the auth...    │   │
│  │ > Reading src/auth.rs...            │   │
│  │                                     │   │
│  │ Full interactive terminal           │   │
│  │ - Colors ✓                          │   │
│  │ - Keyboard input ✓                  │   │
│  │ - Resize ✓                          │   │
│  │ - Full I/O control ✓                │   │
│  └─────────────────────────────────────┘   │
│                                             │
└─────────────────────────────────────────────┘
```

---

## Why Embedded Terminals

### What We Gain

| Capability | External Terminal | Embedded PTY |
|------------|-------------------|--------------|
| Launch agent | ✓ | ✓ |
| Kill agent | ✓ | ✓ |
| Check if alive | ✓ | ✓ |
| **Read output** | ✗ | ✓ |
| **Send prompts programmatically** | ✗ | ✓ |
| **Track conversation state** | ✗ | ✓ |
| **Cross-platform** | ✗ (AppleScript) | ✓ |
| **Single window** | ✗ | ✓ |
| **Enable orchestration** | ✗ | ✓ |

### Why This Matters

1. **Cross-platform**: No more AppleScript. Linux and Windows users can use Shards.

2. **Single window**: All shards in tabs, no window switching.

3. **Foundation for orchestration**: The Orchestrator PRD requires PTY I/O control. This provides it.

4. **Better UX**: See all agents at once, switch with clicks or keyboard.

---

## Why External Terminals First (MVP)

We're NOT building embedded terminals for MVP because:

1. **It's the hardest part**: GPUI + alacritty_terminal + PTY threading is complex
2. **MVP delivers value without it**: Dashboard + external terminals is useful
3. **Validate concept first**: Make sure people want this before the hard work
4. **macOS works fine**: AppleScript terminal launching already works

**Build the simple thing, prove it's valuable, then invest in the hard thing.**

---

## Technical Approach

### The Stack

```
┌─────────────────────────────────────────────┐
│  GPUI (UI Framework)                        │
│  - Window management                        │
│  - Event handling                           │
│  - Rendering                                │
└─────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│  alacritty_terminal (Terminal Emulation)    │
│  - ANSI escape sequence parsing             │
│  - Terminal grid state                      │
│  - Cursor management                        │
│  - Color handling                           │
└─────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│  PTY (Pseudo-Terminal)                      │
│  - Process spawning                         │
│  - stdin/stdout pipes                       │
│  - Signal handling                          │
│  - Platform abstraction                     │
└─────────────────────────────────────────────┘
```

### Why This Stack

**GPUI**: Powers Zed editor. Production-proven, native performance, cross-platform (macOS, Linux, Windows as of 0.2).

**alacritty_terminal**: The terminal emulation library from Alacritty. Handles all the complex ANSI parsing. Zed uses this exact combination.

**PTY**: Platform pseudo-terminal APIs. `pty` crate for Unix, `conpty` for Windows.

### Zed as Reference

Zed editor uses GPUI + alacritty_terminal for its terminal. This proves:
- The stack works
- Performance is good
- Cross-platform is achievable

We can reference Zed's terminal implementation for patterns.

---

## Architecture

### Threading Model

PTY I/O is blocking. Can't do it on the UI thread. Following Zed's pattern:

```
┌──────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│   UI Thread      │     │  PTY Event Loop     │     │   PTY Process    │
│   (GPUI main)    │     │  (background task)  │     │   (claude/bash)  │
│                  │     │                     │     │                  │
│  Renders grid    │◄────│  Batches output     │◄────│  Produces output │
│  Handles input   │────►│  @ 4ms intervals    │────►│  Consumes input  │
│                  │     │                     │     │                  │
└──────────────────┘     └─────────────────────┘     └──────────────────┘
        │                         ▲
        │  keyboard events        │  output events
        └─────────────────────────┘
```

**Key points:**
- Background thread reads PTY output continuously
- Batches output to avoid overwhelming UI (4ms batching like Zed)
- UI thread receives batched events via channel
- Keyboard input sent directly to PTY (low latency path)

### Data Flow

```
User types 'a'
    │
    ▼
GPUI captures KeyDown event
    │
    ▼
TerminalView handles event
    │
    ▼
Writes 'a' to PTY stdin ──────────────────┐
    │                                      │
    ▼                                      ▼
alacritty_terminal echoes 'a'         PTY process
(if echo enabled)                     receives 'a'
    │
    ▼
PTY stdout produces 'a'
    │
    ▼
Background thread reads it
    │
    ▼
Batches and sends to UI thread
    │
    ▼
alacritty_terminal::Term processes
    │
    ▼
Grid state updated
    │
    ▼
GPUI re-renders TerminalView
    │
    ▼
User sees 'a' on screen
```

---

## Key Components

### 1. PtyHandle

Wraps a PTY process with async I/O.

```rust
pub struct PtyHandle {
    /// Write to PTY stdin
    writer: Box<dyn Write + Send>,

    /// Receive events from background reader
    event_rx: mpsc::Receiver<PtyEvent>,

    /// Child process handle
    child: Child,

    /// Current terminal size
    size: PtySize,
}

pub enum PtyEvent {
    /// Output bytes from PTY
    Output(Vec<u8>),

    /// Process exited
    Exit(i32),
}

pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}
```

### 2. Terminal State (alacritty_terminal)

```rust
use alacritty_terminal::term::Term;
use alacritty_terminal::event::EventListener;

pub struct TerminalState {
    /// The terminal grid and state
    term: Arc<FairMutex<Term<EventProxy>>>,

    /// Output buffer for orchestrator queries
    output_buffer: OutputRingBuffer,
}
```

### 3. TerminalView (GPUI)

```rust
pub struct TerminalView {
    /// Terminal state
    state: TerminalState,

    /// PTY handle for I/O
    pty: PtyHandle,

    /// Font metrics for rendering
    font_metrics: FontMetrics,
}

impl Render for TerminalView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        // Read grid from term
        // Render each cell as colored rectangle + text
        // Handle cursor rendering
        // Handle selection rendering
    }
}
```

### 4. PTY Event Loop

```rust
/// Background task that reads PTY and sends events to UI
async fn pty_event_loop(
    mut reader: impl AsyncRead + Unpin,
    event_tx: mpsc::Sender<PtyEvent>,
) {
    let mut buffer = [0u8; 4096];
    let mut batch = Vec::new();
    let batch_interval = Duration::from_millis(4);

    loop {
        tokio::select! {
            // Read from PTY
            result = reader.read(&mut buffer) => {
                match result {
                    Ok(0) => {
                        // EOF - process exited
                        let _ = event_tx.send(PtyEvent::Exit(0)).await;
                        break;
                    }
                    Ok(n) => {
                        batch.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) => {
                        // Handle error
                        break;
                    }
                }
            }

            // Batch timer
            _ = tokio::time::sleep(batch_interval), if !batch.is_empty() => {
                let _ = event_tx.send(PtyEvent::Output(std::mem::take(&mut batch))).await;
            }
        }
    }
}
```

---

## Implementation Phases

### Phase T1: PTY Foundation (No UI)

**Goal**: Spawn PTY, read/write, handle lifecycle. Pure Rust, no GPUI yet.

**Deliverables:**
- `src/pty/mod.rs` - Module structure
- `src/pty/types.rs` - PtyHandle, PtyEvent, PtySize
- `src/pty/spawn.rs` - spawn_pty() function
- `src/pty/event_loop.rs` - Background reader task

**Validation:**
```rust
#[test]
fn test_pty_echo() {
    let mut pty = spawn_pty("/bin/sh", PtySize { rows: 24, cols: 80 })?;
    pty.write(b"echo hello\n")?;

    // Read events until we see "hello"
    let output = collect_output(&pty, Duration::from_secs(2));
    assert!(output.contains("hello"));
}
```

**What NOT to do:**
- No GPUI
- No alacritty_terminal
- Just PTY I/O

---

### Phase T2: Raw Output in Window

**Goal**: Show PTY output in GPUI window. No formatting, just bytes as text.

**Deliverables:**
- Connect PTY to GPUI window
- Display output as scrolling monospace text
- No keyboard input yet

**Validation:**
```bash
cargo run --features ui -- ui
# Window shows shell prompt (with ANSI garbage)
# Proves: PTY spawns, output reaches UI
```

**What NOT to do:**
- No keyboard input
- No ANSI parsing
- No proper terminal rendering

---

### Phase T3: Terminal Rendering

**Goal**: Proper terminal with alacritty_terminal.

**Deliverables:**
- Integrate alacritty_terminal::Term
- Feed PTY output to Term
- Render Term grid in GPUI
- Handle ANSI colors, cursor

**Validation:**
```bash
cargo run --features ui -- ui
# Type: ls --color
# See: Colored output, proper formatting
```

---

### Phase T4: Keyboard Input

**Goal**: Full interactive terminal.

**Deliverables:**
- Capture keyboard events in GPUI
- Translate to terminal escape sequences
- Write to PTY stdin
- Handle special keys (arrows, ctrl, etc.)

**Validation:**
```bash
cargo run --features ui -- ui
# Type: echo "hello world"
# See: hello world
# Use arrow keys, tab completion
# All works
```

---

### Phase T5: Terminal Polish

**Goal**: Production-quality terminal.

**Deliverables:**
- Window resize → PTY resize
- Selection and copy
- Scrollback buffer
- Proper cursor rendering (block, line, etc.)

**Validation:**
- Resize window, terminal adjusts
- Select text, Cmd+C copies
- Scroll up to see history

---

### Phase T6: Integration with Shard System

**Goal**: Replace external terminal launching with embedded.

**Deliverables:**
- "Create Shard" uses embedded PTY instead of AppleScript
- Each shard tab has embedded terminal
- CLI-created shards still show as "external"
- User preference: embedded vs external

**Validation:**
```bash
cargo run --features ui -- ui
# Click [+] to create shard
# Embedded terminal appears with claude running
# Full interactivity
```

---

## Dependencies

### Crates (Feature-Gated)

```toml
[features]
ui = [
    "dep:gpui",
    "dep:alacritty_terminal",
    "dep:parking_lot",
]

[dependencies]
gpui = { version = "0.2", optional = true }
alacritty_terminal = { version = "0.25", optional = true }
parking_lot = { version = "0.12", optional = true }

# For async PTY I/O
tokio = { version = "1", features = ["process", "io-util", "sync"] }
```

### Platform Support

| Platform | PTY | Graphics | Status |
|----------|-----|----------|--------|
| macOS | Native PTY | Metal | Ready |
| Linux | Native PTY | Vulkan | Ready |
| Windows | conpty | DirectX 11 | GPUI 0.2+ |

---

## Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| GPUI breaking changes | High | High | Pin version, prepare for updates |
| Terminal rendering bugs | Medium | Medium | Start simple, iterate |
| PTY platform differences | Medium | Medium | Test early on all platforms |
| Performance (many terminals) | Low | Medium | Lazy rendering, tab suspension |

---

## What This Enables

Once embedded terminals work, we unlock:

1. **Cross-platform**: Linux and Windows support
2. **Single window UX**: All shards in tabs
3. **Orchestrator**: Can read/write to any shard's PTY
4. **Output tracking**: Buffer and query agent output
5. **Automation**: Programmatic agent control

```
MVP Dashboard ──► Embedded Terminals ──► Orchestrator
   (Phases 1-7)      (This PRD)           (Next PRD)
```

---

## Relationship to Other PRDs

```
┌─────────────────────────────────────────────────────────────┐
│                    MVP Dashboard PRD                         │
│                    (Phases 1-7)                              │
│                                                              │
│  - GPUI window ✓                                            │
│  - Shard list ✓                                             │
│  - Create/destroy (external terminals) ✓                    │
│  - Status dashboard ✓                                       │
│  - Favorites ✓                                              │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ builds on
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 Embedded Terminals PRD  ◄── YOU ARE HERE    │
│                 (Phases T1-T6)                               │
│                                                              │
│  - PTY infrastructure                                       │
│  - alacritty_terminal integration                           │
│  - GPUI terminal rendering                                  │
│  - Replace external terminal launch                         │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ enables
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 Orchestrator Agent SDK PRD                   │
│                 (Phases O1-O4)                               │
│                                                              │
│  - AgentHandle wrapping PTY                                 │
│  - send_prompt(), read_output()                             │
│  - Orchestrator with shard skills                           │
│  - Multi-agent coordination                                 │
└─────────────────────────────────────────────────────────────┘
```

---

## Research References

**GPUI:**
- [GPUI Official Site](https://www.gpui.rs/)
- [GPUI crates.io](https://crates.io/crates/gpui) - v0.2.2
- [gpui-component Library](https://longbridge.github.io/gpui-component/)

**alacritty_terminal:**
- [crates.io](https://crates.io/crates/alacritty_terminal) - v0.25.1
- [Alacritty GitHub](https://github.com/alacritty/alacritty)

**Zed Terminal (reference implementation):**
- [Zed Terminal Core Architecture](https://deepwiki.com/zed-industries/zed/9.1-terminal-core)
- [Zed GitHub](https://github.com/zed-industries/zed)

---

## Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| UI framework | GPUI | Proven with Zed, cross-platform |
| Terminal emulation | alacritty_terminal | Battle-tested, Zed uses it |
| Threading | tokio + channels | Async ecosystem, non-blocking |
| State sync | Arc<FairMutex<Term>> | Zed's pattern, prevents starvation |
| Batch interval | 4ms | Zed's tuned value, good balance |

---

*Status: VISION - Post-MVP*
*Created: 2026-01-22*
*Depends on: MVP Dashboard PRD (Phases 1-7)*
*Enables: Orchestrator Agent SDK PRD*
