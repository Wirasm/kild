# KILD User Personas

This document defines who uses KILD and how. All CLI and UI decisions should be informed by these personas.

---

## Overview

KILD has three personas:

1. **The Tōryō (Human)** — The developer running the show. Manages 10-30+ agents across multiple projects.
2. **The Assistant Agent** — An agent on main that helps the Tōryō manage kilds via CLI.
3. **The Worker Agent** — An agent inside a kild, doing focused work in isolation.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         TŌRYŌ (Human)                                   │
│                                                                         │
│   Running 3-6 planning agents on main (brand work, PRDs, research)      │
│   Running 10-20+ worker agents in kilds (building features)             │
│   Semi-interactive: each agent runs 5-120 min, then needs input         │
│   Working across 2-3 projects simultaneously                            │
│                                                                         │
│   THE PROBLEM: The Fog. Losing track of what's running where.           │
└─────────────────────────────────────────────────────────────────────────┘
         │                              │
         │ talks to                     │ monitors
         ▼                              ▼
┌─────────────────────┐    ┌─────────────────────────────────────────────┐
│  ASSISTANT AGENT    │    │              WORKER AGENTS                   │
│  (on main branch)   │    │              (in kilds)                      │
│                     │    │                                              │
│  "Create 5 kilds"   │    │  ┌─────────┐ ┌─────────┐ ┌─────────┐        │
│  "What's running?"  │    │  │ auth    │ │ payments│ │ bugfix  │ ...    │
│  "Stop auth"        │    │  │ kild    │ │ kild    │ │ kild    │        │
│  "Focus payments"   │    │  └─────────┘ └─────────┘ └─────────┘        │
│                     │    │                                              │
│  Manages KILDS,     │    │  Each works in isolation.                    │
│  not the agents     │    │  Tōryō interacts with them directly.         │
│  inside them.       │    │                                              │
└─────────────────────┘    └─────────────────────────────────────────────┘
```

**Key insight:** The Assistant Agent manages the containers (kilds), not the workers inside them. It can create, destroy, stop, focus, and monitor kilds. It cannot talk to the worker agents or know what they're doing beyond what `kild status` shows.

---

## Persona 1: The Tōryō (Human)

**Who:** An agentic-forward engineer running many AI agents in parallel. The master builder who directs, inspects, and decides.

**The Reality:**
- Running 3-6 planning agents on main (research, brand work, PRDs, planning)
- Running 10-20+ worker agents in kilds (executing plans, building features)
- At peak: 30+ total agents across 2-3 projects
- Semi-interactive: agents run 5-120 minutes, then need input
- Constantly cycling attention between terminals

**The Pain (The Fog):**
- "Which terminal is the payment refactor?"
- "Did feature-auth finish or is it waiting for me?"
- "Something failed 20 minutes ago. I didn't notice."
- "I have 6 plan files ready. Which ones have I already started?"
- "I'm running 25 agents and I've lost meaningful awareness of 20 of them."

This is the fog. Context fragments. The cognitive map dissolves. You're more powerful than ever and also operating blind.

**What They Need:**
- Named kilds, not "Terminal 17"
- Status at a glance: `kild list`, `kild health`
- Fast commands: create, stop, destroy, focus
- No friction: no confirmations, no "are you sure?"
- Visibility: what's running, what's stuck, what needs me

**Tool Usage:**
- **CLI:** For quick operations, scripting, agent-assisted management
- **UI (future):** Dashboard for visual overview of all kilds across projects

**Anti-Goals:**
- No confirmation dialogs for routine operations
- No tutorials or onboarding flows
- No hand-holding — they know what they're doing

**Design Implications:**
- Trust the user. Surface errors, don't prevent actions.
- Speed is paramount. Every command should feel instant.
- The fog is the enemy. Every feature should increase visibility.

---

## Persona 2: The Assistant Agent

**Who:** An AI agent running on main that helps the Tōryō manage kilds via CLI.

**What It Is:**
- One of the planning agents the Tōryō is talking to
- Uses the KILD skill to execute commands on the Tōryō's behalf
- The Tōryō says "create 5 kilds for these plans" and the agent runs the commands

**What It Does:**
- Creates kilds: `kild create feature-auth`
- Lists status: `kild list --json`
- Checks health: `kild health --json`
- Stops kilds: `kild stop feature-auth`
- Destroys kilds: `kild destroy feature-auth`
- Focuses terminals: `kild focus feature-auth`
- Opens in editor: `kild code feature-auth`
- Helps with reviews, diffs, inspecting work in kilds

**What It Does NOT Do:**
- Coordinate other agents on main (the Tōryō does that)
- Talk to worker agents inside kilds (they're isolated)
- Know what worker agents are doing beyond kild status
- Act as a "master controller" of anything

**The Relationship:**
```
Tōryō: "Create kilds for auth, payments, and the login bug"

Assistant Agent:
  → kild create feature-auth --note "JWT authentication"
  → kild create feature-payments --note "Refund flow"
  → kild create fix-login-bug --note "Issue #234"
  → kild list --json
  → "Created 3 kilds. All agents running."

Tōryō: "How's auth going?"

Assistant Agent:
  → kild status feature-auth --json
  → "feature-auth is active. Agent running for 23 minutes."

Tōryō: "Stop payments, I need to review it"

Assistant Agent:
  → kild stop feature-payments
  → "Stopped. Worktree preserved. Ready for review."

Tōryō: "Open it in my editor"

Assistant Agent:
  → kild code feature-payments
  → "Opened in zed."
```

**CLI Requirements:**
- `--json` output for parsing
- Non-interactive (no prompts)
- Meaningful exit codes
- Clear error messages

**Design Implications:**
- All commands must work without TTY
- JSON output must be complete (the agent needs full state)
- No interactive prompts ever
- Exit codes must clearly indicate success/failure

---

## Persona 3: The Worker Agent

**Who:** An AI agent (Claude, Kiro, Gemini, etc.) running inside a kild, doing focused work.

**Context:**
- Runs in an isolated worktree (the kild)
- Focused on a specific task: feature, bugfix, plan execution
- The Tōryō interacts with it directly (semi-interactive)
- Runs 5-120 minutes, then needs human input
- Cannot see other kilds or agents

**What It Might Do With KILD CLI:**
- Check its own status: `kild status <own-branch> --json`
- Spawn helper kilds for subtasks (rare): `kild create helper-task`
- Query what exists: `kild list --json`

**What It Cannot Do:**
- Respond to interactive prompts
- Use the UI
- See what's happening in other kilds (by design)

**Design Implications:**
- CLI must work without TTY
- `--json` output for any programmatic use
- Non-destructive defaults (recoverable mistakes)
- Clear exit codes

---

## The Fog: Why KILD Exists

The central problem is not git conflicts. It's not slow builds. It's **losing track**.

When you run 30 agents across 3 projects:
- Which terminal is which?
- What's waiting for input?
- What failed while you were focused elsewhere?
- What's the state of everything?

**KILD's answer:**
1. **Named kilds** — Not "Terminal 17" but `feature-auth`
2. **Status commands** — `kild list`, `kild health`, `kild status`
3. **Focus command** — Bring a specific terminal to front
4. **The dashboard (UI)** — See everything at a glance

The Tōryō must see. The fog must lift.

---

## How Personas Inform Design

| Decision | Tōryō (Human) | Assistant Agent | Worker Agent | Resolution |
|----------|---------------|-----------------|--------------|------------|
| Confirmations | Annoying friction | Cannot respond | Cannot respond | No confirmations; `--force` for destructive ops |
| Output format | Human-readable | JSON required | JSON preferred | Default human; `--json` flag |
| Interactive prompts | Occasionally OK | Never | Never | Avoid prompts; use flags |
| Default behavior | Do what I mean | Safe/recoverable | Safe/recoverable | Non-destructive defaults |
| Speed | Critical | Important | Less critical | Optimize for human perception |
| Visibility | Core need | Needs full state | Limited need | Rich status commands |

---

## CLI Command Checklist

When designing CLI commands, verify:

- [ ] Works without TTY (agents can use it)
- [ ] Has `--json` output (agents can parse it)
- [ ] Has meaningful exit codes
- [ ] No interactive prompts in default path
- [ ] Clear, actionable error messages
- [ ] Fast execution (human won't wait)
- [ ] Minimal required flags

---

## Scale Expectations

KILD is designed for:

| Metric | Current | Expected Growth |
|--------|---------|-----------------|
| Concurrent kilds | 10-20 | 30-50+ |
| Total agents (including main) | 20-30 | 50+ |
| Projects at once | 2-3 | 3-5 |
| Agent run time before input | 5-120 min | Same |

The UI especially must handle 30+ kilds gracefully. The CLI must remain fast regardless of scale.

---

*The Tōryō directs. The Assistant executes commands. The Workers build in isolation. The fog lifts.*
