# Handover — kild × PRP × pi

**Date:** 2026-07-15 · **Status:** the three-layer stack is proven end to end on GPT-5.6 Terra.

This captures the thesis, what is actually built and validated, and what is left — enough
for a cold agent or a future you to pick it up without re-deriving anything.

## The thesis: three layers, one boundary each

| Layer | Owns | Lives in |
|---|---|---|
| **kild** | **the code / the driver** — who runs where, isolation, handoff, how work lands | `~/Projects/mine/kild` |
| **PRP** | **the intelligence** — how work is done: plan → implement → review, gates, validation loops | `~/Projects/prp-spaces/PRPs-agentic-eng` |
| **pi** | **the agent** — cognition: models, sessions, tools, context, auth | upstream (`earendil-works/pi`) |

The rule that makes it work: **each layer ships mechanism, never the layer above's content.**
kild never bakes a personality or a process. PRP never assumes a harness. pi never knows
what a "workstream" is.

Concretely: a kild room spawns pi sessions; each session discovers the PRP skills (via
`~/.agents/skills`); a worker is told *"use the prp-implement skill on this plan"* and the
skill supplies the process. Swap the model, the process holds. Swap the process, the
runtime holds.

### Why this beats the alternatives (settled, don't relitigate)

- **Codex CLI**: skills port fine (same Agent Skills standard) but its subagent control
  plane is an undocumented black box, and it has **no worktree isolation** — parallel
  agents share one checkout. Kept as a zero-maintenance render target (below), not the
  orchestration lane.
- **A pi subagent extension**: pi ships no subagents by design ("build your own"). kild's
  engine already *is* that, with better observability. Don't build a second one.
- **kild is the orchestration lane.** Its control plane is code we own, not prose we hope
  another harness interprets.

## Built and validated (2026-07-15)

Dogfooded live, GPT-5.6 Terra (`openai-codex/gpt-5.6-terra`), a two-stage pipeline in one
kild room, ~$0.20 total:

1. **Skills reach pi**: all 11 PRP skills discovered in a pi session (`~/.agents/skills`
   symlinks → the PRP repo's `.agents/skills/`), alongside personal skills, no duplicates.
2. **Skills reach kild**: a kild session (`kild run`) executed `prp-commit` end to end;
   with `--worktree`, inside `kild/<name>`, main checkout untouched.
3. **The full pipeline through a room**: `orchestrator` + `worker` participants →
   **prp-plan** wrote a real plan → I gated it as the human → steered the *same room* into
   **prp-implement** → implementation committed. Independently verified: 6/6 unit tests
   pass, `add 2 3`→`5`, `subtract 5 3`→`2`, invalid input exits 2 with usage.
4. **The protocol holds**: the worker reported with evidence (commit SHA + validation
   output); the orchestrator **independently verified before believing** (confirmed the
   plan commit contained no implementation); standing decisions were respected; no
   unauthorized scope, no PR/push when told not to.
5. **The plan was stack-correct**: the model replaced the plan template's TypeScript
   examples with `python3 -m py_compile` / `python3 -m unittest`, marked the DB/browser
   validation levels "Not applicable" instead of hallucinating MCP servers, and cited real
   `MIRROR` refs (`calc.py:1-6`). The PRP template's "illustrative examples, replace them"
   framing survives a model change.

### Changes made to kild (uncommitted, in your tree)

- **pi SDK 0.78.1 → 0.80.7** (`bun update` leaves `pi-coding-agent` behind — pin it
  explicitly). Typecheck/lint/48 tests green on the new SDK.
- **Room kickoff addressing bug, fixed** (`engine/src/cli.ts`). The kickoff used an inline
  `/@[A-Za-z0-9_-]+/` to decide whether to prepend `@<lead>`. That asks *"does this text
  mention anyone?"* when the only question that matters is *"does it address a
  **participant**?"* — and `@human` is never a participant, yet it is exactly what a goal
  says when it names who to report back to. Any well-formed orchestration goal therefore
  addressed nobody and **the room sat idle forever** (a live 25-minute stall, found by
  dogfooding). Now uses `parseMentions` (the module documented as "the one place that
  decides who a post addresses", which the CLI was bypassing) and tests against the actual
  participant list.
- **Duplicate addressing authority, removed** (`engine/src/kild/room/room-router.ts`). The
  manager resolved `to` at post time (`opts.system ? [] : (opts.to ?? parseMentions(text))`),
  then the router **re-derived it from the text** whenever `to` was empty — a second,
  divergent answer to the same question that silently overrode a deliberate empty `to`.
  Live consequences: inviting `@worker` prompted the worker with a turn saying *"@worker
  joined the room."*, and halting a 1:1 room prompted the agent you just halted with
  *"Room halted by the operator."* The `RoomMessage.system` field existed and the router
  never read it. Now: **the manager answers "addressed to whom?", the router owns only
  delivery policy** (never `@human`, never self, the 1:1 bare-post rule, never for
  notices/implicit replies). Regression tests added for both live bugs.
- **Agent personalities rewritten** (`.pi/agents/orchestrator.md`, `.pi/agents/worker.md`)
  — the PRP orchestration protocol as prompt: decomposition with definition-of-done,
  evidence-based reporting, verify-before-believe, standing decisions vs. gate digests,
  one-room-one-workstream, blocked-worker escalation that resumes the *same* worker.

## Known gaps (ranked)

1. ~~**No `close_room`.**~~ **Done (2026-07-16).** The lead participant now holds a
   `close_room` tool (symmetric with `invite_agent`): control line → engine posts a
   system notice, stops every participant, archives the room; the `kild room` CLI
   resolves on the archive broadcast. Engine-side lead-only enforcement (a control
   line is just stdout — the engine, not the subprocess, is the authority). Live-proven:
   a delegation+verify+report room self-terminated in 20s with no operator action.
2. ~~**A post addressing only unknown handles dies silently.**~~ **Done (2026-07-16,
   ws-no-recipient):** unknown handles now get a system notice naming the room's actual
   participants. **Still open — the sibling failure (live 2026-07-17):** an OPERATOR post
   (human/brain) in a multi-participant room with zero participant mentions is broadcast-only
   and gives no feedback — the brain's mention-less gate approval stalled a room for an hour.
   Fix with the SAME primitive: extend the existing notice to cover the no-addressee case for
   non-participant senders. Queued behind slice 8a (same file, room-manager).
3. ~~**Personalities only resolve globally via `~/.claude/agents`.**~~ **Done (2026-07-16,
   ws-global-agents):** `$KILD_HOME/agents` is discovered with tested precedence.
4. **The Flue layer is off the hot path.** Its frozen, explicitly invoked experiments
   include run/auth/brain demos and the worktree-sandbox (see the 2026-07-15
   runtime-integration research in the PRP repo), but nothing on the session hot path uses
   them and the dogfood never touched them. Do not extend them until the fleet layer names
   a real server or CLI endpoint. Note `brain.ts` already exposes open-room/post-to-room
   tools — it is the seed of the fleet layer.
5. **Worktree convention overlap.** kild owns worktree policy (`$KILD_HOME/worktrees/`,
   `kild/<name>`); the PRP pack's `prp-worktree` skill uses in-repo `.worktrees/`. In the
   kild lane **kild's mechanism wins** — workers must not invoke `prp-worktree`. Worth one
   line in the orchestrator personality so the conventions never mix.

## Next moves

**In kild** — (a) `close_room` as a lead-held tool; (b) decide the no-recipient policy;
(c) `~/.kild/agents` discovery; (d) Flue: park or justify.

**The fleet layer — SHIPPED and live-proven (2026-07-16).** The brain is a kild session
(`kild fleet "<goal>"`, agent `brain`, `KILD_FLEET=1`) whose tools — `open_room`,
`post_room`, `rooms_status`, `close_room` — are HTTP clients of the same engine REST
surface the cockpit drives (new: `POST /api/rooms[,/:id/post,/:id/close]`, participant-
aware kickoff addressing, non-room WS `spawn/prompt/stop` restored). Memory is a durable
run ledger (`.kild/fleet/`, maintainer-style atomic writes). First run: the brain opened
a room, the room's orchestrator/worker did the PRP work and self-closed, the brain caught
a wobble mid-run and steered it back, verified the commit against git, and closed its
ledger `Status: Complete`. The Flue `brain.ts` is now superseded — deleting it is the
obvious next cleanup. **Fleet run 2 (2026-07-17, multi-workstream):** the brain drove audit slices 4 and P2-persistence
through two serialized rooms on one shared branch (its own overlap call, SD-2), surviving a
three-round review with a human-escalated hard stop, and kept an exemplary ledger (SD-1..5,
incidents, independent verifications). Merged: `2fd9938` (lifecycle states, durable closed-state,
atomic history writes; 87 tests). **Slice 8 requirements, each with a live reproduction from this
run:** (1) room gate/close events must PUSH to the room's opener — the brain is event-blind between
turns and prompt-level polling loops do not survive turn economics (two stalls, two manual wakes);
(2) an operator post addressing no participant needs the unknown-recipient treatment — a
mention-less gate approval was broadcast-only and stalled a room for an hour; (3) the brain's
digest-to-human channel must be durable — its session stream dies with the spectator CLI;
(4) free-form `from` let the brain label itself `fleet-brain` (slice 5's actor model).

**In the PRP repo** — the Codex render (`.agents/skills/`, `.codex/agents/*.toml`,
generated by `scripts/sync_plugin.py`, `--check` in CI) stays as-is: zero maintenance,
serves both pi and Codex from the same tree. Live validation of the `codex exec` JSONL
parser in `prp_loop.py --cli codex` is still pending a working codex binary. **Do not**
author a Codex-specific orchestrator — kild is that lane.

## Landmines

- `git status` hides `.codex/` and `.agents/` in most editors — the TOMLs *are* committed.
- Skills live in `~/.agents/skills` (shared standard); Codex custom agents live in
  `~/.codex/agents` (proprietary). Both are symlinked to the PRP repo, so
  `sync_plugin.py` propagates globally with no reinstall.
- pi docs promise symlink-following for **skill folders**; for **agent TOMLs** it is
  undocumented. If Codex custom agents don't appear, copy instead of link.
- Answering *"who is this addressed to?"* by pattern-matching prose has now bitten three
  separate places (kild's kickoff, the router, and a monitor script I wrote — two of the
  three got it wrong). Resolve addressing **once**, at the source, and pass it as data.
