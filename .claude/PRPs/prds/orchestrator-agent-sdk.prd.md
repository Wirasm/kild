# Rust-Native Agent Orchestrator for Shards

**Status**: VISION - Pre-MVP research capture
**Depends on**: Embedded Terminals (Phase 8+ of UI PRD)
**Timeline**: Post-MVP

---

## Meta: Why This Document Exists

This PRD captures research and vision for a future orchestration layer. We're documenting this now because:

1. We did the research and don't want to lose it
2. It informs architectural decisions in earlier phases
3. It explains WHY we're building embedded terminals (they enable this)

**This is not ready for implementation.** Build the MVP dashboard and embedded terminals first.

---

## The Vision

A main orchestrating agent that can:
- Spawn child agents in separate shards
- Send prompts to specific running agents
- Read output from any agent
- Coordinate work across multiple agents
- All from a single interface

```
User: "Start a shard for auth and one for the API refactor"

Orchestrator:
→ Creates shard "auth" with Claude Code
→ Creates shard "api-refactor" with Claude Code
→ Both appear as tabs in UI

User: "Tell the auth shard to add JWT validation"

Orchestrator:
→ Finds auth shard's PTY handle
→ Writes "Add JWT validation to the auth module" to PTY stdin
→ Auth agent starts working
→ Orchestrator can monitor progress via PTY stdout

User: "What's the status of api-refactor?"

Orchestrator:
→ Reads recent output from api-refactor's PTY
→ Summarizes progress to user
```

---

## Why This Matters

### Current State (Fire-and-Forget)
- Shards launches agents in external terminals
- Zero control after launch
- Can't read output, can't send commands
- Human must manually switch windows
- No coordination between agents

### Future State (Full Orchestration)
- Embedded terminals = full PTY control
- Orchestrator can inject prompts into any agent
- Orchestrator can read any agent's output
- Single interface for multi-agent coordination
- Foundation for autonomous workflows

---

## Key Insight: The SDK is Just Subprocess + JSON

### What We Learned

The Claude Agent SDK (TypeScript/Python) does NOT use special APIs. It:

1. **Spawns Claude Code CLI** as a subprocess
2. **Communicates via stdin/stdout** using line-delimited JSON
3. **Tracks conversation state** (just a list of messages)
4. **Handles streaming** (reads JSON lines as they arrive)
5. **Allows custom system prompts** (CLI flag or config)

**That's it.** The SDK is a thin convenience wrapper.

### The JSON Protocol

```bash
# Claude Code CLI supports headless mode
claude -p "Your prompt" --output-format stream-json

# Each line of stdout is a JSON object
{"type": "assistant", "content": "I'll help you..."}
{"type": "tool_use", "tool": "Read", "params": {...}}
{"type": "tool_result", "result": "..."}
{"type": "result", "content": "Done!", "session_id": "abc123"}
```

### Session Management

```bash
# Resume a session
claude -p "Continue" --resume "session-id"

# Control tool permissions
claude -p "Do task" --allowedTools "Read,Edit,Bash(git:*)"
```

---

## Why Rust, Not TypeScript SDK

We considered two approaches:

### Option A: TypeScript SDK via Node.js (Rejected)

```
Shards UI (Rust) ←→ IPC ←→ Node.js + SDK ←→ Claude CLI
```

**Problems:**
- Node.js runtime dependency
- Two processes, two languages
- IPC complexity
- Startup latency
- Can't ship single binary

### Option B: Rust-Native Implementation (Chosen)

```
Shards UI (Rust) → Orchestrator (Rust) → Claude CLI
```

**Benefits:**
- Single binary, no external dependencies
- Direct GPUI integration
- Same language as codebase
- No IPC overhead
- Full control

**The SDK doesn't do anything Rust can't do.** It's subprocess + JSON parsing - trivial in Rust.

---

## Architecture

### Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Shards UI (GPUI)                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    ORCHESTRATOR                             │ │
│  │                                                             │ │
│  │  - Custom system prompt (knows about shards)                │ │
│  │  - Skills: spawn_shard, send_to_shard, read_shard, etc.    │ │
│  │  - Runs as in-process Rust module                          │ │
│  │  - Manages AgentHandle instances for each shard            │ │
│  │                                                             │ │
│  └────────────────────────────────────────────────────────────┘ │
│         │                    │                    │              │
│         ▼                    ▼                    ▼              │
│  ┌─────────────┐      ┌─────────────┐      ┌─────────────┐     │
│  │ Shard Tab 1 │      │ Shard Tab 2 │      │ Shard Tab 3 │     │
│  │             │      │             │      │             │     │
│  │ Embedded PTY│      │ Embedded PTY│      │ Embedded PTY│     │
│  │      ↓      │      │      ↓      │      │      ↓      │     │
│  │ claude      │      │ claude      │      │ codex       │     │
│  │ (interactive│      │ (interactive│      │ (interactive│     │
│  │  full CLI)  │      │  full CLI)  │      │  full CLI)  │     │
│  │             │      │             │      │             │     │
│  │ /commands ✓ │      │ /commands ✓ │      │ full CLI ✓  │     │
│  │ subagents ✓ │      │ skills ✓    │      │             │     │
│  └─────────────┘      └─────────────┘      └─────────────┘     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Key Components

**1. Orchestrator (Rust module)**
- Custom Claude agent with shard-management skills
- Tracks all running agent handles
- Routes commands to specific shards
- Aggregates information from shards

**2. AgentHandle (per-shard)**
- Wraps a PTY process running `claude` or `codex`
- Provides: `write(prompt)`, `read_output()`, `is_alive()`
- Buffers output for querying

**3. Embedded PTY Terminals**
- Full interactive CLI agents (not `-p` headless mode)
- Support slash commands, subagents, skills, MCP
- Human can also interact directly
- Orchestrator injects via PTY stdin

---

## How Orchestration Works

### Sending a Prompt to a Shard

```rust
// Orchestrator receives: "Tell auth shard to add tests"
let shard = self.shards.get("auth")?;
shard.pty_handle.write(b"Add tests for the authentication module\n")?;

// The text appears in the PTY as if user typed it
// Claude Code in that terminal processes it normally
```

### Reading Shard Output

```rust
// Orchestrator receives: "What's auth shard doing?"
let shard = self.shards.get("auth")?;
let recent_output = shard.output_buffer.last_n_lines(50);

// Orchestrator can summarize this for the user
// Or parse it for status information
```

### The Orchestrator's System Prompt

```markdown
You are the Shards Orchestrator. You coordinate multiple AI coding agents.

Available skills:
- spawn_shard(name, repo, agent) - Create new shard with agent
- send_to_shard(name, prompt) - Send prompt to running shard
- read_shard(name, lines) - Get recent output from shard
- list_shards() - List all active shards
- destroy_shard(name) - Stop and remove shard

When user asks to coordinate work:
1. Spawn shards for different tasks
2. Send appropriate prompts to each
3. Monitor progress by reading output
4. Report status back to user
```

---

## Why Embedded Terminals are Required

**This orchestration ONLY works with embedded PTY terminals.**

| Capability | External Terminal | Embedded PTY |
|------------|-------------------|--------------|
| Launch agent | ✓ | ✓ |
| Kill agent | ✓ | ✓ |
| Check if alive | ✓ | ✓ |
| Read output | ✗ | ✓ |
| Send prompts | ✗ | ✓ |
| Track conversation | ✗ | ✓ |
| Orchestrate | ✗ | ✓ |

External terminals (iTerm, Ghostty) are fire-and-forget. We launch the process but have no handle to its I/O.

Embedded PTY gives us full control: we own the stdin/stdout pipes.

---

## Why Full Interactive Mode (Not `-p`)

The `-p` flag runs Claude Code in headless mode, but loses features:

| Feature | `-p` headless | Full interactive |
|---------|---------------|------------------|
| Basic prompts | ✓ | ✓ |
| Slash commands | ✗ | ✓ |
| Subagents | ✗ | ✓ |
| Skills | ✗ | ✓ |
| MCP servers | ✗ | ✓ |
| Human can intervene | ✗ | ✓ |
| Session persistence | Limited | Full |

By running full interactive Claude Code in an embedded PTY:
- Agents have full capabilities
- Human can still interact directly with any shard
- Orchestrator injects prompts via PTY stdin (same as typing)

---

## Rust Implementation Sketch

### Core Types

```rust
/// Handle to a running agent in a PTY
pub struct AgentHandle {
    pub shard_name: String,
    pub agent_type: AgentType,  // Claude, Codex, etc.
    pty: PtyHandle,             // From embedded terminal phase
    output_buffer: OutputRingBuffer,
    session_id: Option<String>,
}

impl AgentHandle {
    /// Send a prompt to this agent (writes to PTY stdin)
    pub fn send_prompt(&mut self, prompt: &str) -> Result<()> {
        self.pty.write(prompt.as_bytes())?;
        self.pty.write(b"\n")?;
        Ok(())
    }

    /// Get recent output from this agent
    pub fn recent_output(&self, lines: usize) -> Vec<String> {
        self.output_buffer.last_n(lines)
    }

    /// Check if agent process is still running
    pub fn is_alive(&self) -> bool {
        self.pty.is_alive()
    }
}

/// The main orchestrator
pub struct Orchestrator {
    agents: HashMap<String, AgentHandle>,
    system_prompt: String,
}

impl Orchestrator {
    /// Create a new shard with an agent
    pub async fn spawn_shard(&mut self, name: &str, config: ShardConfig) -> Result<()> {
        // 1. Create worktree (existing shards-core logic)
        // 2. Spawn PTY with `claude` or `codex`
        // 3. Create AgentHandle
        // 4. Store in self.agents
    }

    /// Send prompt to a specific shard
    pub fn send_to_shard(&mut self, name: &str, prompt: &str) -> Result<()> {
        let agent = self.agents.get_mut(name)?;
        agent.send_prompt(prompt)
    }

    /// Read recent output from a shard
    pub fn read_shard(&self, name: &str, lines: usize) -> Result<Vec<String>> {
        let agent = self.agents.get(name)?;
        Ok(agent.recent_output(lines))
    }
}
```

### Output Buffering

```rust
/// Ring buffer for agent output
pub struct OutputRingBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
}

impl OutputRingBuffer {
    pub fn push(&mut self, line: String) {
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn last_n(&self, n: usize) -> Vec<String> {
        self.lines.iter().rev().take(n).rev().cloned().collect()
    }
}
```

---

## Implementation Phases (Future)

These phases come AFTER embedded terminals are working.

### Phase O1: Basic Agent Handle

- Wrap PTY in AgentHandle struct
- Implement send_prompt() and recent_output()
- Test: spawn claude, send prompt via code, verify it executes

### Phase O2: Orchestrator Core

- HashMap of AgentHandles
- spawn_shard(), send_to_shard(), read_shard(), list_shards()
- No UI yet - test via code

### Phase O3: Orchestrator UI Tab

- Dedicated orchestrator tab in UI
- Custom system prompt with shard skills
- User talks to orchestrator, it manages shards

### Phase O4: Polish

- Better output parsing (detect agent idle/thinking)
- Error handling (agent crashed, prompt failed)
- Status aggregation across shards

---

## Open Questions

- [ ] How do we detect when an agent is "idle" vs "thinking"?
- [ ] Should orchestrator be a separate Claude instance or just a mode?
- [ ] How to handle agent errors/crashes gracefully?
- [ ] Should output buffer be bounded or paginated?
- [ ] How to surface orchestrator skills (custom MCP server?)

---

## Research Sources

**Claude Agent SDK:**
- [Python SDK](https://github.com/anthropics/claude-agent-sdk-python)
- [TypeScript SDK](https://github.com/anthropics/claude-agent-sdk-typescript)
- [Building agents with Claude Agent SDK](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk)
- [Headless mode docs](https://code.claude.com/docs/en/headless)

**Key findings:**
- SDK spawns CLI as subprocess
- Communication via stdin/stdout JSON
- Line-delimited JSON protocol
- Session management via `--resume`
- Tool permissions via `--allowedTools`

**Codex CLI:**
- Similar subprocess model
- `codex exec` for programmatic use
- MCP server mode available

---

## Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| SDK approach | Rust-native | Single binary, no Node.js dependency, trivial to implement |
| Agent mode | Full interactive (not `-p`) | Preserves slash commands, subagents, skills, MCP |
| Orchestrator location | In-process Rust | Direct GPUI integration, no IPC |
| PTY requirement | Embedded terminals required | External terminals don't give I/O control |

---

## Relationship to Other PRDs

```
MVP Dashboard PRD (Phases 1-7)
    │
    │ builds foundation
    ▼
Embedded Terminals (Phase 8+)
    │
    │ enables orchestration
    ▼
THIS PRD: Orchestrator Agent SDK ← you are here
```

**Don't build this until embedded terminals work.** But keep this vision in mind - it's why we're building embedded terminals in the first place.

---

*Status: VISION - Research captured*
*Created: 2026-01-22*
*Depends on: Embedded Terminals PRD (to be written)*
