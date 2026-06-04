# KILD — Vision & Mission

*Draft: January 2026 — Updated: February 2026*

---

## Mission

**Make parallel AI development accessible to everyone.**

Today, only senior engineers successfully manage multiple AI agents working in parallel. The cognitive overhead of coordinating agents, understanding git worktrees, managing terminal sessions, and preventing conflicts creates an expertise barrier that locks out most developers.

KILD removes that barrier. We provide the isolation, visibility, and control that makes parallel agent workflows feel as natural as managing browser tabs.

---

## Vision

**A world where developers direct fleets of AI agents as naturally as they manage browser tabs today.**

The future of software development isn't one human and one AI assistant. It's one human directing many AI agents — each working on a different task, in a different isolated environment, all visible at a glance.

We're building the control layer for that future.

---

## The Problem We Solve

### The Fog

When you run multiple AI agents simultaneously, you lose track:

- "Which terminal was the payment refactor?"
- "Did feature-auth finish or is it waiting for me?"
- "Something failed 20 minutes ago. Where?"
- "I have 6 tasks running. How many are actually progressing?"

This is **the fog** — the cognitive fragmentation that comes from parallel work without parallel visibility.

The fog is the enemy. KILD lifts the fog.

### The Expertise Barrier

Git worktrees are the standard isolation mechanism for parallel agents. Cursor, Windsurf, Conductor — they all use worktrees under the hood.

But worktrees are a power-user feature. Most developers have never used them. The mental model is unfamiliar. The commands are obscure. The result: parallel agent workflows remain inaccessible to most developers.

KILD abstracts the complexity. You don't need to understand worktrees. You just need to say "create a kild for this task" and start working.

---

## Strategic Positioning

### What KILD Is

- **An isolation layer** — Each agent gets its own universe. They can't step on each other.
- **A visibility layer** — See all your agents at a glance. Know what's running, stuck, or waiting.
- **A control layer** — Create, stop, destroy, focus. Manage your fleet with simple commands.
- **An intelligence layer** — Know which branches conflict, which are ready to merge, which agents are stepping on each other's files.
- **A landing layer** — Rebase, sync, queue, merge. Get parallel work back into main safely and efficiently.
- **Agent-agnostic** — Works with Claude, Cursor, Kiro, Gemini, Codex, or any future agent.
- **IDE-agnostic** — Not locked to one editor. Works with your existing tools.

### What KILD Is Not

- Not an AI agent framework (we manage where agents run, not how they think. Even with embedded terminals, the AI runs in the terminal — KILD manages the runtime, not the model)
- Not a replacement for git (git is the foundation; we build intelligence on top of it)
- Not locked to one ecosystem (works across agents, editors, terminals, and isolation methods)

---

## The Wedge

**Isolation is the foundation. Intelligence is the differentiator.**

You can't have 10 agents without isolation. They'll conflict, overwrite each other's work, and create chaos.

You can't have 100 agents without visibility. You'll lose track, miss failures, and waste compute on stuck tasks.

You can't land 30 branches without intelligence. You'll drown in merge conflicts, miss file overlaps, and waste hours on manual rebasing.

KILD provides all three. That's the wedge.

---

## The Expansion Path

### Phase 1: Power Users (Now)

**The Tōryō** — Senior engineers running 10-30+ agents across multiple projects.

These users already understand the problem. They're already using worktrees manually, managing terminal sessions, losing track of what's running. KILD gives them the tool they've been building themselves with bash scripts and discipline.

**What they need:**
- Fast CLI with `--json` output for scripting and external tooling (Waybar, status bars, dashboards)
- Named sessions with status tracking
- Agent status reporting via hooks (working, idle, waiting, done)
- Health monitoring and focus commands
- Non-interactive, agent-friendly interface
- Git intelligence: diff stats, ahead/behind, cross-kild file overlaps

### Phase 2: Broader Developers (Next)

**The Rising Tide** — Developers adopting parallel workflows as agents improve.

As AI agents become more capable, more developers will want to run multiple agents. But they won't learn git worktrees. They need the complexity hidden behind excellent UX.

**What they need:**
- Native GUI that makes parallel agents visual and intuitive
- One-click creation, simple status views
- Merge readiness indicators and intelligent merge queue
- No terminal required for basic operations — embedded terminal in the GUI
- Gentle learning curve, powerful when needed

### Phase 3: Vibe Coders (Future)

**The New Majority** — Developers who direct agents without deep technical knowledge.

These users don't want to understand isolation, branches, or worktrees. They want to say "build me these 5 features" and have it work. They judge tools by how they feel, not how they're implemented.

**What they need:**
- Delightful UX that hides all complexity
- Smart defaults that just work
- Pluggable isolation (containers, VMs) — they don't care how it works, just that it does
- Automated merge queue that lands parallel work without manual intervention
- Visual feedback that builds confidence
- The power of parallel agents without the cognitive overhead

---

## Why Now

### The Market Is Moving Fast

- **Cursor 2.0** ships with parallel agents and git worktree support
- **Windsurf Wave 13** launched 5-agent parallel sessions
- **Conductor** built an entire product around worktree-based agent orchestration
- **85% of developers** now regularly use AI tools for coding (up from ~40% in 2023)

The parallel agent pattern is no longer experimental. It's becoming the default workflow for productive teams.

### The Bottleneck Has Shifted

The DORA 2025 report found PR review time increased 91%. Code generation is no longer the bottleneck — review and coordination are.

When you can generate code 10x faster, the limiting factor becomes:
1. How many parallel workstreams can you manage?
2. How quickly can you review and merge results?

KILD addresses #1 directly and #2 through git intelligence (cross-kild awareness, merge readiness, intelligent merge queue).

### The Expertise Barrier Is Real

"So far, only senior engineers are successfully managing parallel agent workflows. Junior developers struggle with the cognitive overhead."

This barrier is an opportunity. Whoever makes parallel agents accessible to everyone captures a massive market as agent capabilities improve.

---

## The Moat

### Brand

"KILD" is distinctive, memorable, and ownable. The mythology (Honryū, Tōryō, the Fog) creates emotional resonance that commodity tools lack.

### Agent-Agnostic

We're not locked to Claude, Cursor, or any single ecosystem. As new agents emerge, KILD works with all of them. The isolation layer is agent-independent.

### Git Intelligence

No other tool understands the relationships between parallel branches. KILD knows which kilds share modified files, which will conflict, and what order to merge them. This intelligence compounds — the more kilds you run, the more valuable it becomes.

### UX Focus

Our competition is building worktree support as a feature. We're building parallel agent management as the product. That focus means better UX, better edge case handling, better polish.

### Dual Integration Surface

`kild-core` is a Rust library. The CLI and GUI both consume it. External tools can integrate via CLI `--json` output (for any language) or import `kild-core` directly (for Rust). This means the community can build on KILD without us being the bottleneck.

### Community

Apache 2.0 licensing means developers can use, modify, and contribute. The community becomes a moat as adoption grows.

---

## Success Metrics

### Near-term (6 months)
- 1,000+ active CLI users
- GUI in production use
- Recognition in AI developer tooling discussions

### Medium-term (12-18 months)
- 10,000+ active users across CLI and GUI
- Premium features generating revenue
- Partnerships or integrations with major AI tools

### Long-term (24+ months)
- Standard tool for parallel agent workflows
- Acquisition target for major AI/dev tooling company, or
- VC-backed expansion to full agent orchestration platform

---

## The Architecture Layers

KILD's architecture expands in layers, each independently useful:

```
Layer 5: Merge Queue          Automated landing of parallel work
Layer 4: Git Intelligence      Cross-kild awareness, conflict prediction, merge ordering
Layer 3: Agent Awareness       Status reporting, health monitoring, activity tracking
Layer 2: Visibility            Status, health, diff stats, enriched JSON for tooling
Layer 1: Isolation             Worktrees (now), containers/VMs/devcontainers (future)
         ─────────────────────────────────────────────────────────────────────────
         Foundation: Git        Branches, worktrees, diffs, merges — git does the heavy lifting
```

**The runtime**:
- Today: External terminals (Ghostty, iTerm, Alacritty) spawn agent processes
- Future: Session daemon (tmux-like persistent process manager) + embedded terminal in GUI
- In all cases: KILD manages the runtime. The AI agent runs inside it. KILD never touches the AI directly.

**Isolation backends** (today and future):

| Backend | Isolation Level | Phase |
|---------|----------------|-------|
| Git worktree | Branch-level filesystem | Now |
| Container (Docker/Podman) | Filesystem + network + process | Future |
| VM (Firecracker/QEMU) | Full hardware-level | Future |
| Devcontainer | Standardized container spec | Future |

Worktrees remain the default. Other backends are opt-in for users who need stronger isolation.

**Run from anywhere**:

The daemon architecture unlocks remote access. The same daemon that runs locally over a unix socket can run on a VPS over HTTPS/gRPC:

```
Local:     CLI/GUI → unix socket → kild daemon (your machine)
Remote:    CLI/GUI → HTTPS/gRPC → kild daemon (VPS/cloud)
Mobile:    App     → HTTPS/gRPC → kild daemon (VPS/cloud)
```

Deploy KILD on a beefy VPS. Agents run in containers on that machine. You're the Tōryō from anywhere — your laptop, your phone, a tablet on the train. The fleet keeps working while you're away. You check in, approve merges, kill stuck agents, spin up new kilds.

**Interact from anywhere**:

When the daemon owns the PTY (pseudo-terminal) rather than delegating to external terminals, it can multiplex agent I/O to any connected client:

```
kild daemon (owns PTY)
├── agent process reads stdin, writes stdout (doesn't know who's connected)
└── clients:
    ├── embedded terminal in kild-ui (local)
    ├── kild attach (like tmux attach, any terminal)
    └── mobile app (streams stdout, injects stdin over network)
```

On mobile, a chat-style UI is more natural than a terminal emulator. Agent interaction is mostly text-in/text-out:

- Agent: "Implemented auth flow. Add tests or move to API endpoint?"
- Push notification hits your phone
- You type: "Add tests first"
- Agent continues

The Tōryō directs the fleet from anywhere. Monitor status, approve merges, and interact with agents — from your laptop, your phone, or a tablet on the train.

---

## The One-Liner

**KILD: Parallel agents. Isolated worlds. One sane developer.**

---

*Fracture the Honryū.*
