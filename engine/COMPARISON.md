# kild on Flue vs kild in Rust — a grounded comparison

> **Historical decision record.** Since this was written, the cockpit UI moved out
> of kild into its own project ([helm](https://github.com/Wirasm/helm)); kild is
> engine + CLI + pi-extension, and its REST/WS API is the contract any UI consumes.
> Cockpit/frontend mentions below reflect the state at decision time.

This spike rebuilds kild's current capabilities **and** the three batteries kild
doesn't have yet, on Flue, and runs all of them against a real model. Everything
below was executed end-to-end (see `README.md` for the exact commands and their
output). The goal is a real decision, not a vibe.

## What was proven (all ran live)

| # | Capability | Result |
|---|---|---|
| — | Project registry (`projects.ts`) | ✅ rebuild of `kild-core::project` |
| — | Agent convention-dir loading (`agents.ts`) | ✅ `.kild/.claude/.pi` discovery, frontmatter strip |
| — | One-shot `kild run` → `RunOutcome` (`run.ts`) | ✅ model/text/tokens/cost, project cwd, named agent |
| 1 | **Worktree-as-Sandbox** (`worktree.ts`) | ✅ agent shell/fs ran inside a real `git worktree`; file landed on disk; branch isolated |
| 2 | **Agent rooms / peer comms** (`rooms.ts`) | ✅ two independent agents held a real proposal→concern→ack conversation through a kild channel |
| 3 | **Brain (operator mirror)** (`brain.ts`) | ✅ agent orchestrated via kild capability-tools and posted status to a room |
| ★ | Merge agent team (`merge-team-demo.ts`) | ✅ 3 reviewers in parallel + conflict-aware merge order |

## The headline: lines of code for the same capability

| Area | Rust kild | Flue spike | Why |
|---|---:|---:|---|
| Agent runtime boundary (`rpc`) | **583** | **0** | pi-agent-core is in-process; `await session.prompt()` replaces subprocess spawn + JSONL framing + event translator + split writer |
| projects + agents + run | 328 | 171 | same logic, less ceremony |
| CLI surface | 282 | (workflows) | Flue's `flue run` + HTTP/WS routing is the surface |
| **Rebuild subtotal** | **~1,193** | **~342** | ~3.5× |
| Worktree battery | — | 55 | not built in Rust yet |
| Rooms battery | — | 80 | not built in Rust yet |
| Brain battery | — | 128 | not built in Rust yet |
| 5 demo workflows | — | 223 | proofs |
| **Total** | **1,193** (rebuild only) | **657** (rebuild **+** 3 batteries **+** demos) | |

The single biggest fact: **the entire `rpc` slice evaporates.** kild's hardest,
most pi-coupled code — spawning `pi --mode rpc`, LF-framed JSONL, the
`PiOutput`→domain translator, the split read/write halves, EPIPE-draining
shutdown — is 583 lines that exist only because kild talks to pi *across a
process boundary*. Flue embeds pi-agent-core, so that boundary is a function call.

## Where Flue clearly wins

1. **In-process kernel kills the rpc tax.** No subprocess, no JSONL, no translator.
   `runToCompletion` is 20 lines vs a ~600-line slice.
2. **The brain is natural.** kild's capabilities are TS functions; the operator-mirror
   agent calls them directly as tools (`list_projects`, `create_worktree`,
   `run_agent_in_worktree`, `post_to_room`). In Rust, the brain agent would have to
   shell out to the `kild` CLI and re-expose every capability as a tool anyway — so
   the capability surface *wants* to live in the agent's runtime, which is TS.
3. **Batteries slot into real seams.** Worktree = `local({ cwd })` behind Flue's
   `SandboxFactory`. Rooms = three `defineTool()`s over an in-process bus. Both took
   tens of lines and worked first try after typecheck.
4. **Structured output built in.** valibot `result:` schemas gave typed verdicts in the
   merge team with zero parsing.
5. **Deploy-anywhere + parallel delegation** (`session.task()`, separate harnesses)
   come with the framework — kild's daemon/VPS story, partly handled.

## Where Rust / pi-CLI still wins (the honest cons)

1. ~~**Auth regression.**~~ **RESOLVED (proven live).** Flue's runtime doesn't read
   `~/.pi/agent/auth.json` *by default*, but the bridge is ~30 lines (`src/kild/auth.ts`):
   `getOAuthApiKey()` from `@earendil-works/pi-ai/oauth` refreshes the stored token and
   `configureProvider()` hands it to pi-ai, whose anthropic provider auto-detects the
   `sk-ant-oat…` OAuth token and adds the Claude Code beta headers. Verified: both
   `anthropic/claude-haiku-4-5` (Claude Max) and `openai-codex/gpt-5.5` (ChatGPT)
   run through the user's OAuth subscriptions. `flue run auth-test`. No longer a con.
2. **No crash isolation.** pi-agent-core runs in the Node process. One agent that
   wedges or OOMs can take the host process with it. kild's subprocess-per-session
   gives crash isolation for many parallel agents — a real property for a fleet.
3. **Young + churning.** Flue is `0.9.2`, banner-flagged *"Experimental — APIs may
   change."* Its pi deps are pinned `*`. Betting the engine on it means riding breakage.
4. **Weaker type guarantees + heavier distribution.** Rust's type system and
   single-binary, no-runtime distribution vs TS + a Node runtime.
5. **Session concurrency is serialized.** Parallel branches need separate
   harnesses/sessions (the merge team uses one harness per PR). Fine, but a constraint
   to design around.

## Net read

For kild's **stated vision** — a cockpit orchestrating coding-agent *teams*, with
rooms, a merge team, GitHub reviews, and a learning operator-mirror brain — Flue is
a strong fit and removes the most code from the hardest place. The brain alone is
the tell: it wants kild's capabilities in its own runtime, which pulls the whole
orchestration layer into TS regardless.

The two cons that actually matter are **auth** (the user's OAuth subscriptions) and
**crash isolation** (a fleet property). Neither is a showstopper — auth is almost
certainly bridgeable via the shared pi-ai layer, and crash isolation can be regained
by running agents in worktree-bound `local()` sandboxes or, for scale, remote
sandboxes (Daytona) — but both deserve a deliberate answer before committing.

What does **not** change in either world: the **cockpit UI is kild's moat and still
must be built**, and the **Svelte frontend + design system port unchanged.**
