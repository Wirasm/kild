# KILD — Brand Bible v2

*Draft: January 2026*

---

## Part 1: What KILD Actually Is

KILD is a CLI (and eventually GUI) for running parallel AI coding agents in isolated git worktrees.

**In plain terms:** You're working on a project. You have six things to do. Instead of doing them one at a time, you spin up six isolated copies of your codebase, dispatch an AI agent into each one, and let them all work in parallel. When they're done, you review and merge. The agents can't step on each other. Your main branch stays clean. You stay sane. Mostly.

**The real workflow it enables:**
- 3-6 planning agents on main (researching, writing docs, creating plans)
- 10-20+ execution agents in kilds (building features in parallel)
- Peak load: 30+ agents across 2-3 projects. Expecting more.
- All semi-interactive (they run 5-120 minutes, then need you)
- You alt-tab between them, trying to remember which is which

That last part is the problem KILD solves.

---

## Part 2: The Enemy

### The Fog (霧 — Kiri)

The enemy is not git conflicts. The enemy is not slow builds. The enemy is **losing track**.

When you run thirty parallel workstreams across three projects:
- "Which terminal was the payment refactor?"
- "Did feature-auth finish or is it waiting for me?"
- "Something failed 20 minutes ago. I think."
- "I have six plan files. How many have I started? Three? Four?"
- "Wait, which project was that even in?"

You're more powerful than you've ever been. You're also operating blind. Congratulations on your productivity gains.

**This is the fog.** Your cognitive map fragments. Context scatters. You're technically running ten agents but you've lost meaningful awareness of eight of them.

KILD cuts through the fog. Each workstream has a name. Each has a state. The dashboard shows you everything at a glance. You know where to look.

---

## Part 3: The Origin

*A forge tale for the age of artificial intelligence.*

*(Yes, we're doing mythology for a CLI tool. Stay with us.)*

---

### I. The Honryū

Before KILD, there was only the main branch — the **Honryū (本流)**, the main current.

Developers approached it carefully. To change one line was to risk breaking the flow. They worked slowly. They called this "being responsible."

Then the agents arrived.

---

### II. The Agents

Suddenly you could summon ten minds, twenty minds, thirty minds, each capable of writing code at inhuman speed.

The problem: they were chaos. Two agents on the same codebase meant two agents editing the same file. Context bled between tasks. Experiments polluted the Honryū before they were ready. The agents were powerful. They were also unsupervised toddlers with root access.

Developers tried guardrails. Sandboxes. Elaborate prompting. Rules upon rules.

It helped. Mostly. Sometimes. Not really.

---

### III. The Cut

The answer was simpler: **separation**.

With a single command, you cut a piece from the Honryū — a **kild**. This kild is not a copy. It's a living branch with its own directory, its own ports, its own terminal. A pocket universe.

Into this pocket universe, you dispatch an agent.

"Work," you say. "Break things. Rewrite history. Go wild. The Honryū can't feel it."

The agent works. Fast, reckless, free.

When it's done, you review. If it's good, you fuse the kild back into the Honryū. If it's bad, you destroy it. Either way, the main current was never at risk.

This is **structural isolation**. This is KILD.

---

### IV. The Tōryō (棟梁)

In Japanese tradition, the **Tōryō** is the master builder. They don't swing every hammer. They direct, inspect, decide. They see the whole while others focus on parts.

When you run KILD, you are the Tōryō.

You create kilds. You dispatch agents. You monitor their health — which are working, which are idle, which have crashed while you were getting coffee. You decide what merges back and what gets destroyed.

In practice, this means you're alt-tabbing between terminals while mass-reviewing PRs. The mythology is aspirational.

---

### V. The Creed

**We believe in the Cut.**
Each task deserves its own universe. Context-switching between branches is for people who enjoy suffering.

**We believe in Isolation.**
An agent loose in the Honryū is an agent creating problems you'll find in production. Containment isn't constraint — it's sanity.

**We believe in the Return.**
We fracture to focus. But we always fuse back. The kild returns to the Honryū, and the current is stronger.

**We believe in Sight.**
The Tōryō must see. `kild list`. `kild health`. The fog lifts. You know what's running, what's stuck, what needs you. This is the bare minimum for not losing your mind.

---

## Part 4: Terminology

We use real terms from Japanese and Estonian. Not because we're pretentious (okay, a little), but because the standard tech vocabulary is exhausted. "Orchestrator." "Pipeline." "Workflow." These words mean nothing anymore.

| Concept | Term | Origin | What It Means |
|---------|------|--------|---------------|
| The main branch | **Honryū (本流)** | Japanese | "Main current." The source. What you protect. |
| An isolated worktree | **Kild** | Estonian | A shard. A splinter. A piece cut from the whole. |
| The developer | **Tōryō (棟梁)** | Japanese | Master builder. Directs, inspects, decides. |
| The AI worker | **Agent** | English | It's fine. Not everything needs a special name. |
| The enemy | **The Fog** | English/Nordic | Loss of awareness. Operating blind. |

**In practice:** Use "kild" everywhere. Use "Honryū" and "Tōryō" sparingly — they're for brand voice, not everyday conversation. No one wants to say "I'm Tōryō-ing my Honryū."

---

## Part 5: The Commands

| Command | What It Does | The Mythology |
|---------|--------------|---------------|
| `kild create <name>` | Create worktree + branch + terminal + agent | Make the cut. |
| `kild list` | Show all kilds and their states | Survey the field. |
| `kild status <name>` | Inspect one kild in detail | Focus your attention. |
| `kild health` | Dashboard with metrics for all kilds | The Tōryō's view. |
| `kild open <name>` | Spawn new agent in existing kild | Send in reinforcements. |
| `kild stop <name>` | Kill agent, keep worktree | Pause, don't destroy. |
| `kild destroy <name>` | Remove everything | The work is done (or abandoned). |
| `kild focus <name>` | Bring terminal to front | Look here. |
| `kild code <name>` | Open in editor | Inspect directly. |

---

## Part 6: Visual Identity

### The Aesthetic: Tallinn Night

We don't do "friendly." We don't do "playful gradients." We do **cold, sharp, precise**.

The visual language is Nordic winter meets Japanese discipline:
- Deep voids (the long dark)
- Sharp edges (the blade, the ice)
- Minimal ornamentation (function over decoration)
- Subtle texture (frosted glass, obsidian)

**This is not a toy.** The aesthetic should feel like a serious tool for serious work. The humor lives in the copy, not the visuals.

---

### Color Palette

#### Dark Theme (Default) — "Tallinn Night"

| Name | Hex | Usage |
|------|-----|-------|
| Void | `#050505` | Deepest background |
| Obsidian | `#0A0A0C` | Panels, sidebars |
| Electric Cornflower | `#3B82F6` | Primary action, the Cut |
| Agent Purple | `#8B5CF6` | AI activity indicator |
| Frost White | `#EDEDEF` | Primary text |
| Muted | `#71717A` | Secondary text |

#### Light Theme — "Baltic Ice"

| Name | Hex | Usage |
|------|-----|-------|
| Glacial | `#FFFFFF` | Background |
| Zinc Mist | `#F4F4F5` | Panels |
| Deep Baltic | `#2563EB` | Primary action |
| Iron Black | `#09090B` | Primary text |

---

### Typography

- **UI:** Inter (clean, neutral, professional)
- **Code/Terminal:** JetBrains Mono (the only correct choice)

---

### Iconography: Crystal Wire

Icons are **1.5px stroke, sharp angles, no fills**.

- Kild icon: Diamond or split hexagon (a shard)
- Project icon: Stacked panes (layers)
- Branch icon: Jagged fracture line

---

## Part 7: Voice & Copywriting

### The Voice

Cold. Precise. Slightly amused.

Think: a ship's computer that's seen some things. It doesn't panic. It doesn't celebrate. It states facts with dry clarity. Occasionally, it has opinions.

### The Rules

**Be direct.** Don't pad sentences. Don't soften bad news.

```
❌ "Oops! It looks like something might have gone wrong with the agent."
✅ "Agent down."
```

**Be specific.** Vague is useless.

```
❌ "Starting up..."
✅ "Creating worktree at ~/.kilds/feature-auth"
```

**Be dry, not dead.** A little personality is allowed.

```
❌ "ERROR: Process terminated."
✅ "Agent stopped responding. It happens."
```

```
❌ "Here are your workspaces:"
✅ "Kilds: 4 active, 2 idle, 1 crashed while you weren't looking."
```

**Acknowledge the absurdity.** We're using samurai metaphors for git worktrees. We know.

```
✅ "You're the Tōryō now. (Master builder. Not a typo.)"
```

---

### Error Messages

Errors should be:
1. Clear about what happened
2. Helpful about what to do
3. Not apologetic (we didn't do anything wrong)
4. Occasionally wry

```
Agent unreachable. Check if it's still running, or if it wandered off.

Worktree already exists. Pick a different name, or destroy the existing one first.

Branch 'main' is protected. You can't kild the Honryū. (That's the point.)

No kilds found. Either you haven't created any, or something has gone very wrong.
```

---

### Taglines

Primary: **Fracture the Honryū.**

Alternatives:
- "Parallel agents. Isolated worlds. One sane developer."
- "Cut. Dispatch. Merge. Repeat."
- "Thirty agents. Zero collisions. Some visibility."
- "The fog lifts."

---

## Part 8: The Name

**Kild** (/kɪlt/) — Estonian for "shard."

We chose Estonian because:
- It's where the tool was born (Tallinn)
- The word is short, sharp, and memorable
- It's not already a JavaScript framework

The word means a splinter of ice, a fragment of glass. Something cut from a larger whole. This is exactly what the tool does.

---

## Part 9: What This Isn't

KILD is not:
- **An orchestration platform.** (We refuse to use that word.)
- **An AI agent framework.** We manage where agents run, not how they think.
- **A replacement for git.** Git does the actual branching. We wrap it.
- **Enterprise software.** No dashboards with 47 metrics. No "observability." One developer, many agents, clarity.

KILD is:
- A precision tool for parallel AI work
- Isolation so you can go fast without breaking things
- Visibility so you don't lose track
- A CLI that respects your time

---

## Part 10: Future Vision

The GUI (coming) will be called **KILD** as well. Same name, visual interface.

The CLI remains the primary interface for power users. The GUI is for monitoring — the Tōryō's dashboard. See all kilds at a glance. Click to focus. Watch the health. The fog, visualized and dispelled.

---

*Fracture the Honryū.*

*Or, in plainer terms: run your agents in parallel without losing your mind.*
