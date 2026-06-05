# kild — Vision

## Mission

Let one operator direct a whole team of coding agents — plan the work, let them
build it in isolation, and land it — without losing the thread.

Code generation is no longer the bottleneck. **Managing many parallel
workstreams and landing them safely is.** kild is the control layer for that.

## What kild is

**An opinionated orchestrator for pi/Flue agent teams.** One operator directs
many agents — each isolated, all visible, the work landed.

The operator is **either a human or an orchestrator agent that stands in for
them.** Both drive the *same* surface: invoke an agent, author a new one, prompt
a running one, hand off agent-to-agent, watch the fleet, land the work.

The "brain" is not special infrastructure. It is just an agent holding kild's
orchestration tools — so a human directing the fleet and an agent directing the
fleet are the same motion.

## The boundary

This is what makes kild opinionated without becoming a prompt framework:

- **kild owns orchestration** — who runs where, how they hand off, how work lands.
- **pi owns cognition** — how agents think: the model, the loop, the tools, auth.
- **Prompts are data, not code** — every agent personality, including the
  orchestrator's own, lives in `.pi/agents` / `.claude/agents`, authored and
  edited by humans *and* agents. kild ships mechanism and, at most, a default
  system prompt. It never bakes personalities into the codebase.

## The bet

kild is **committed to pi and Flue — not agnostic.** The moat is depth, not
breadth: the deepest pi-native orchestration and Flue-backed isolation and
landing there is, contributing upstream as we go.

## The enemy: the Fog

Run many agents at once and you lose the thread — which one is stuck, which is
waiting on you, which finished, which two are about to collide. That cognitive
fog is the enemy. kild lifts it: the fleet visible at a glance, conflicts seen
before they happen, work landed in order.

## The operator

**The Tōryō** — one director, many agents. Today a human at the cockpit.
Increasingly, an orchestrator agent the human supervises. The director directs;
the fleet builds; kild keeps it sane.

---

**kild — direct the fleet. Isolated worlds. Work that lands.**

*Fracture the Honryū.*
