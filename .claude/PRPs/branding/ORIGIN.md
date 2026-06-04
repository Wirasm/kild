# THE ORIGIN OF KILD

*A mythology for a CLI tool. Yes, really.*

---

## How to Read This Document

This is the **origin story** — the mythological framing for KILD. It's meant to give the brand emotional weight and a distinctive voice.

**The balance:**
- The mythology is real branding. Use it in marketing, the website, the README intro.
- The self-aware humor is also real. We know we're writing samurai metaphors for git worktrees.
- When in doubt: mythic for inspiration, practical for documentation.

**Key terms established here:**
- **Honryū (本流)** — The main branch. The main current.
- **Kild** — An isolated worktree. A shard cut from the Honryū.
- **Tōryō (棟梁)** — The developer. The master builder who directs.
- **The Fog** — The enemy. Losing track of what's running where.

If someone asks "why is it called Honryū?" — this document is why.

---

## I. THE HONRYŪ

Every codebase has a main branch — the **Honryū (本流)**, the main current.

The Honryū is the source of truth. Everything flows from it. Everything returns to it. Developers approach it with appropriate caution. Break the Honryū, break the build. Break the build, break the deploy. Break the deploy, break production. Break production, update your LinkedIn.

For years, this was fine. You branched, you worked, you merged. One task at a time. Slow, careful, controlled.

Then the agents arrived.

---

## II. THE AGENTS

Suddenly you could summon ten minds, twenty minds, thirty minds — each capable of writing code at inhuman speed.

The productivity gains were real. So was the chaos.

Two agents on the same codebase meant two agents editing the same file. Context bled between tasks. Experiments leaked into the Honryū before they were ready. The agents were powerful. They were also unsupervised interns with commit access.

Developers tried guardrails. Sandboxes. Elaborate prompting. "Please don't touch the auth module." The agents nodded politely and touched the auth module.

There had to be another way.

---

## III. THE FOG

But the agents weren't the only problem.

When you run thirty parallel workstreams across three projects, something else happens: **you lose track**.

- "Which terminal was the payment refactor?"
- "Did feature-auth finish or is it waiting for me?"
- "Something failed 20 minutes ago. Where?"
- "I have six plan files ready. How many have I started?"
- "I'm running 30 agents and I've lost meaningful awareness of 25 of them."

This is **the Fog**. Your cognitive map dissolves. Context fragments. You're more powerful than you've ever been and also operating blind.

The agents weren't the enemy. The fog was.

*(Look, we could have called it "context fragmentation" or "cognitive overhead." But "the Fog" sounds better in a mythology document. This is branding.)*

---

## IV. THE CUT

The answer was separation.

In the depths of a Baltic winter — Tallinn, specifically, where the nights are long and the debugging is longer — the solution emerged:

**Don't protect the Honryū. Cut it.**

With a single command, you separate a piece — a shard, a *kild* — from the main current. This kild is not a copy. It's a **living branch** with its own directory, its own ports, its own terminal. A pocket universe.

Into this pocket universe, you dispatch an agent.

"Work," you say. "Break things. Rewrite history. Go absolutely wild. The Honryū can't feel it."

The agent works. Fast, reckless, free.

When it's done, you review. If it's good, you fuse the kild back into the Honryū. If it's bad, you destroy it. Either way, the main current was never at risk.

And because every kild has a **name**, a **status**, a **place** — you can see them all. The fog lifts.

This is **structural isolation**. This is KILD.

---

## V. THE TŌRYŌ

In Japanese tradition, the **Tōryō (棟梁)** is the master builder.

The Tōryō doesn't swing every hammer. They direct the craftsmen. They inspect the work. They see the whole while others focus on parts. They decide what gets built and what gets torn down.

When you use KILD, you are the Tōryō.

You create kilds. You dispatch agents. You monitor their health — which are working, which are idle, which have crashed while you were getting coffee. You decide what merges back and what gets destroyed.

In practice, this means you're alt-tabbing between 30 terminals while mass-reviewing PRs. The mythology is aspirational. But aspirational is the point.

*(The Japanese term is real, by the way. Master carpenters in traditional construction. We're not making this up. Well, we're making up the part where it applies to CLI tools.)*

---

## VI. THE DISCIPLINE

To wield KILD is to practice a discipline. Here's the lifecycle:

### The Cut
You make the incision. `kild create`. One command, one new universe. The worktree splits from the Honryū. The terminal opens. The agent awakens.

### The Work
Each kild is an isolated workspace. You can run thirty at once — each agent in their own kild, each working toward a different goal. They cannot interfere. They cannot collide. The Honryū sleeps undisturbed.

### The Watch
The Tōryō must see. `kild list`. `kild health`. `kild status`. You monitor your kilds — their state, their progress, which need attention. This is how the fog lifts. This is the dashboard.

### The Pause
When you need to stop — `kild stop`. The agent process dies. But the kild persists: worktree intact, work-in-progress preserved. You can return later. `kild open`. The work continues.

### The Fusion
When the work is ready, you merge it back. Git does what git does. The kild returns to the Honryū. The seam is visible — every commit tracked — but the current is stronger.

### The Release
When all is done, you destroy the kild. `kild destroy`. The worktree vanishes. The ports are freed. The terminal closes. Only the merged work remains.

---

## VII. THE CREED

**We believe in the Cut.**
Each task deserves its own universe. Context-switching between branches in the same directory is suffering. We don't fear the fracture — we control it.

**We believe in Isolation.**
An agent loose in the Honryū is an agent creating problems you'll find in production. Containment isn't constraint — it's freedom. Inside the kild, the agent can break anything. Outside, the Honryū is pristine.

**We believe in Sight.**
The Tōryō must see. The fog is the enemy. Every feature of KILD exists to increase visibility: named kilds, status commands, health dashboards, focus commands. If you don't know what's running, you're not in control.

**We believe in the Return.**
We fracture to focus. But we always fuse back. The kild returns to the Honryū, and the current is stronger. The long winter ends. What we built in isolation serves us in the light.

---

## VIII. THE NAME

**Kild** (Estonian: /kilt/) — a shard. A splinter of ice. A fragment of broken glass.

We chose this word because:
- It's sharp
- It's distinct
- It's not already a JavaScript framework

The word was born in the Baltic, where winters are long and dark, where the ice cracks with a sound like thunder.

We brought to it the discipline of the Japanese master builder — the Tōryō who sees the whole, the single decisive cut, the understanding that to build something strong you must first separate it from the chaos.

Nordic ice. Japanese precision. Estonian vocabulary.

*(We're Swedish, living in Estonia, borrowing Japanese concepts for a Rust CLI tool. Globalization is weird.)*

---

## IX. THE VOICE

When KILD speaks, it speaks like a ship's computer. Cold. Precise. Occasionally dry.

```
> Creating kild: feature-auth
> Worktree: ~/.kilds/shards/feature-auth
> Branch: kild_8f2a3b
> Agent dispatched: claude
> Kild active.
```

We don't say "Starting up..." We say **"Creating kild."**

We don't say "Error occurred." We say **"Agent unreachable."**

We don't say "Here are your workspaces." We say **"Kilds: 4 active, 2 stopped, 1 crashed while you weren't looking."**

The voice is cold. The voice is certain. The voice is the long winter speaking. The voice occasionally has opinions.

---

## X. THE FUTURE

Today, KILD is a CLI. You run `kild create`, `kild list`, `kild destroy`.

Tomorrow, KILD will have a GUI — the Tōryō's dashboard. All your kilds at a glance. All your projects. Click to focus. Watch the health. The fog, visualized and dispelled.

The scale will grow. Thirty agents today. Fifty tomorrow. A hundred next year. The tools must keep up.

But the core remains:
- **Cut** — Create isolated universes
- **Work** — Let agents build in freedom
- **See** — Never lose track
- **Return** — Fuse back to the Honryū

---

*Fracture the Honryū.*

*Or, in plainer terms: run your agents in parallel without losing your mind.*

---

## Appendix: Quick Reference

| Term | Meaning | Why This Word |
|------|---------|---------------|
| **Kild** | Isolated worktree | Estonian for "shard" |
| **Honryū** | Main branch | Japanese for "main current" |
| **Tōryō** | The developer | Japanese for "master builder" |
| **The Fog** | Losing track | Because "cognitive fragmentation" is boring |
| **The Cut** | `kild create` | The decisive action |
| **Fusion** | Merging back | Kintsugi vibes |

---

*Document version: January 2026*
*To be read with appropriate amounts of irony.*
