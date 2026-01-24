# Shards User Personas

This document defines the primary users of Shards. Both CLI and UI design decisions should be informed by these personas.

---

## Overview

Shards has three personas, but they're really two categories:

1. **Humans** - Power users who interact via CLI or UI
2. **Agents** - AI agents that use CLI programmatically
   - **Worker Agents** - Run inside shards, do focused work
   - **Main Agent (Orchestrator)** - Runs on main, coordinates all shards

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           USER (Human)                                   │
│                     "spin up shards for X, Y, Z"                        │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      MAIN AGENT (Orchestrator)                          │
│                         Runs on main branch                             │
│                                                                         │
│   • Understands natural language from user                              │
│   • Has context via: shards list --json                                 │
│   • Orchestrates via CLI: shards create/open/stop/destroy               │
│   • Monitors shard status and reports back                              │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
            │                       │                       │
            ▼                       ▼                       ▼
┌───────────────────┐   ┌───────────────────┐   ┌───────────────────┐
│   WORKER AGENT    │   │   WORKER AGENT    │   │   WORKER AGENT    │
│   (Shard: auth)   │   │  (Shard: payments)│   │  (Shard: bugfix)  │
│                   │   │                   │   │                   │
│   Focused work    │   │   Focused work    │   │   Focused work    │
│   in isolation    │   │   in isolation    │   │   in isolation    │
└───────────────────┘   └───────────────────┘   └───────────────────┘
```

---

## Persona 1: Power User (Human)

**Who**: Agentic-forward engineers who run multiple AI agents simultaneously and need clean environment separation.

**Context**:
- Solo developer managing parallel AI workflows
- Works across multiple repositories
- Comfortable with terminal, git, and CLI tools
- Values speed and control over hand-holding
- Knows what they're doing - doesn't need warnings for obvious actions

**Goals**:
- Spin up isolated workspaces quickly
- See all active shards at a glance
- Switch between shards without context-switching friction
- Clean up shards when done

**Anti-goals**:
- Doesn't want confirmation dialogs for routine operations
- Doesn't want "are you sure?" prompts that slow them down
- Doesn't need tutorials or onboarding flows

**Tool usage**:
- **CLI**: Primary interface for scripting, quick one-off shards, headless/CI workflows
- **UI**: Dashboard for visual overview, managing many shards, favorites

**Design implications**:
- Trust the user - surface errors, don't prevent actions
- Fast paths for common operations
- Keyboard-first UI (vim-inspired shortcuts)
- No unnecessary friction or confirmation steps

---

## Persona 2: Main Agent (Orchestrator)

**Who**: An AI agent running on the main branch that coordinates all shards via CLI.

**Context**:
- Runs in a terminal on the main branch (not in a shard)
- Acts as the user's "control center" for shard management
- User gives natural language commands, agent translates to CLI
- Has full visibility into all shards via `shards list --json`
- Can spawn, monitor, and clean up shards as needed

**Example interaction**:
```
User: "Spin up shards for the auth feature and the payments bug"
Main Agent:
  → shards create feature-auth --agent claude
  → shards create fix-payments-bug --agent claude
  → "Created 2 shards. Both agents are running."

User: "How's the auth work going?"
Main Agent:
  → shards list --json
  → "feature-auth is running, fix-payments-bug is running"

User: "Stop the payments shard, I need to review it"
Main Agent:
  → shards stop fix-payments-bug
  → "Stopped. Shard preserved at ~/.shards/worktrees/..."

User: "Clean up the auth shard, it's merged"
Main Agent:
  → shards destroy feature-auth
  → "Destroyed feature-auth shard"
```

**Goals**:
- Translate natural language to CLI commands
- Maintain context about what's running: `shards list --json`
- Spawn shards on demand: `shards create <branch> --agent <agent>`
- Add agents to existing shards: `shards open <branch> --agent <agent>`
- Stop agents when requested: `shards stop <branch>`
- Clean up completed work: `shards destroy <branch>`
- Report status back to user in natural language

**Anti-goals**:
- Cannot respond to interactive prompts
- Should not need to parse human-readable output (needs `--json`)

**Design implications**:
- `shards list --json` must provide complete state for orchestration
- All commands must be non-interactive by default
- Exit codes must clearly indicate success/failure
- Error messages should be actionable

---

## Persona 3: Worker Agent (In-Shard)

**Who**: AI agents (Claude, Kiro, Codex, etc.) running inside shards doing focused work.

**Context**:
- Runs inside a terminal in a shard (isolated worktree)
- Focused on a specific task (feature, bugfix, etc.)
- May spawn helper shards for subtasks
- May stop itself when work is complete
- Operates programmatically - no interactive prompts

**Goals**:
- Focus on assigned task in isolated environment
- Optionally spawn helper shards: `shards create helper-task --agent claude`
- Stop self when done: `shards stop <own-branch>`
- Query status if needed: `shards list --json`

**Anti-goals**:
- Cannot respond to interactive prompts (y/n confirmations)
- Cannot use the UI
- Should not need `--force` flags for normal operations

**Tool usage**:
- **CLI only** - agents don't use the UI
- Needs machine-readable output (`--json` flag)
- Needs non-interactive mode by default

**Design implications**:
- CLI must work without TTY/interactive prompts
- Provide `--json` output for programmatic parsing
- Exit codes should be meaningful and documented
- Error messages should be parseable
- Default behavior should be non-destructive (so agents can recover from mistakes)

---

## How Personas Inform Design

| Decision | Power User | Main Agent | Worker Agent | Resolution |
|----------|------------|------------|--------------|------------|
| Confirmations | Annoying friction | Cannot respond | Cannot respond | No confirmations; `--force` only for dangerous ops |
| Output format | Human-readable | Machine-readable | Machine-readable | Default human-readable, `--json` flag for agents |
| Error handling | Clear message | Parseable + exit code | Parseable + exit code | Both: clear message AND proper exit code |
| Interactive prompts | Rarely acceptable | Never | Never | Avoid prompts; use flags instead |
| Default behavior | Do what I mean | Safe/recoverable | Safe/recoverable | Non-destructive defaults |
| State visibility | `shards list` | `shards list --json` | `shards list --json` | Both formats available |

---

## CLI Command Design Checklist

When designing CLI commands, verify against both personas:

- [ ] Works without TTY (agent can use it)
- [ ] Has `--json` output option (agent can parse it)
- [ ] Has meaningful exit codes (agent can check success/failure)
- [ ] No interactive prompts in default path (agent won't hang)
- [ ] Clear error messages (human can understand)
- [ ] Fast execution (human won't wait)
- [ ] Minimal required flags (human won't type extra)

---

## Main Agent Orchestration Pattern

The Main Agent pattern enables natural language control of shards:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  User: "Create shards for auth, payments, and the login bug"            │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Main Agent (on main branch):                                           │
│                                                                         │
│  1. Parse intent: create 3 shards                                       │
│  2. Execute:                                                            │
│     → shards create feature-auth --agent claude                         │
│     → shards create feature-payments --agent claude                     │
│     → shards create fix-login-bug --agent claude                        │
│  3. Verify: shards list --json                                          │
│  4. Report: "Created 3 shards. All agents running."                     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key CLI requirements for this pattern**:
- `shards list --json` - Complete state for decision-making
- `shards create` - Non-interactive, returns immediately
- `shards open` - Additive, can add multiple agents
- `shards stop` - Clean shutdown without destroying
- `shards destroy` - Clean removal

**The Main Agent is just a regular agent** running on main. It doesn't need special privileges or a different interface - it just uses the same CLI that worker agents and humans use. The power comes from:
1. Running on main (sees the whole picture)
2. Having conversation context with the user
3. Translating natural language to CLI commands

---

*This document should be referenced when designing new CLI commands or UI features.*
