# DEMOLITION — the agent-centric teardown

**Companion to the design brief** (`kild — the agent-centric model`). Read that first for the
model; this is the file-by-file kill list against the current tree. Brutal on purpose. The
only consumers are our own UI (helm) and the CLI we drive, so **nothing needs backwards-compat**
— we delete, we don't deprecate.

Legend: **KEEP** = load-bearing mechanism, stays · **RESHAPE** = survives but loses its
room/operator skin · **MOVE** = intelligence, belongs in `wirasm/prp` · **DIE** = deleted.

### Two invariants this teardown must preserve

- **`spawn` does not imply a worktree.** `spawn(agent, into: <kild>)` is the DEFAULT — another
  agent in the *same* tree (`@coder` + `@reviewer` in `kild/auth`, seeing each other's
  uncommitted work). `spawn(agent, fork: <base>)` is the deliberate, rarer act that creates a
  new kild = a new worktree. Many agents per worktree is the normal case; `worktree` stays
  optional on a kild (the root `kild-0` is just the main checkout with agents in it).
- **Messaging is peer-to-peer.** `send` has no privileged sender or recipient — any agent can
  write any agent whose handle it can name. The honryo is one more handle, not a hub. This is
  exactly what dies with `room-router.ts`: today an unaddressed post silently routes to
  `participants[0]` and a `@human` report force-wakes the lead, so peer messaging exists but
  bends through an engine-imposed hierarchy.

---

## Scoreboard

Two independent demolitions, not one:

1. **The dead Flue lane** — a whole subsystem the live engine never calls. Pure dead weight,
   deletable today, zero behaviour change. ~400 source lines + a dependency.
2. **The Room model** — the operator/lead/@human/decisions choreography the design brief
   targets. ~500 lines deleted outright, plus `room-manager.ts` collapsing from **809 → ~250**,
   and matching cuts in `worker.ts`, `server.ts`, `room-types.ts`.

Rough engine total: **~6,700 source lines → a third deleted**, and the single biggest file
(`room-manager.ts`) more than halves. ~1,400 test lines go with them.

---

## Demolition 1 — the dead Flue lane (delete today, unrelated to the reframe)

The engine (`server.ts` → `worker.ts`) resolves models through pi's `ModelRuntime` directly.
Nothing on that path imports `@flue/runtime`. Everything below is a frozen demo or a
superseded runner. HANDOVER already flagged it; it's worse than legacy — it's a dependency
carried for dead code.

| File | Lines | Why it dies |
|---|---|---|
| `engine/src/kild/run.ts` | 49 | Flue in-process one-shot runner. Imported only by `workflows/run.ts`. The real `kild run` goes through the worker. **DIE** |
| `engine/src/kild/auth.ts` (`bridgePiAuth`) | 54 | Bridges pi auth → Flue's `configureProvider`. Imported only by `workflows/auth-test.ts`. The live engine auths via `ModelRuntime.create()` in `worker.ts`. **DIE** |
| `engine/src/flue/worktree-sandbox.ts` | 66 | Flue `SandboxFactory`. Used only by `workflows/worktree-demo.ts`. The real worktree mechanism is `kild/worktree.ts`. **DIE** (or actually upstream it to Flue and drop from here — see `docs/upstream-worktree-sandbox.md`, itself now **DIE**) |
| `engine/src/workflows/*` (hello, auth-test, run, worktree-demo, merge-team-demo) | ~180 | Frozen demos; `workflows/README.md` says so itself. **DIE** |
| `engine/flue.config.ts` | — | Flue harness config. **DIE** |
| `@flue/runtime` dependency | — | Drops out of `engine/package.json` once the above are gone. **DIE** |
| `engine/src/flue/worktree-sandbox.test.ts` | 48 | Test for a deleted module. **DIE** |

**Net:** ~400 source + 48 test lines and one dependency, removable in a single commit with no
behaviour change to the running engine.

---

## Demolition 2 — the Room model teardown

### Deleted outright

| File | Lines | Verdict |
|---|---|---|
| `kild/room/room-router.ts` | 105 | The whole addressing choreography — lead default, `@human`-wakes-lead, implicit-reply suppression, 1:1 bare-post rule. Directed `send(to)` replaces it with a ~10-line delivery loop. **DIE** |
| `kild/room/room-decisions.ts` | 88 | The `needs-decision[key]` / `resolved[key]` ledger. A *protocol*, not mechanism. **MOVE → PRP** |
| `kild/room/room-events.ts` | 44 | Operator-notification formatting, `finalNonSystemPost`, `humanPostEvent`, opener notify. All the "tell the operator" plumbing. **DIE** |
| `kild/room/room-lifecycle.ts` | 58 | 4-state machine + 6 guards (`opening/running/halted/closed`). A kild is open or landed. Collapses to near-nothing. **DIE** (reshape to a trivial open/closed flag if anything) |
| `kild/room/rest-room-attribution.ts` | 70 | `from`-vs-`human` actor resolution across open/post/close. The *principle* (engine-derived identity) survives on the one `send` endpoint; this three-headed room-shaped version **DIES**. |
| `kild/operator/open-room-tool.ts` | 43 | Operator tier. **DIE** |
| `kild/operator/post-room-tool.ts` | 27 | Operator tier. **DIE** |
| `kild/operator/rooms-status-tool.ts` | 26 | Operator tier. **DIE** |
| `kild/operator/close-room-tool.ts` | 35 | Operator tier. **DIE** |
| `kild/room/close-room-tool.ts` | 43 | Lead-only `close_room` worker tool. Closing becomes a seat power, not a lead's tool. **DIE** (reshape into an optional `land` behind a PRP gate) |
| `kild/room/invite-agent-tool.ts` | 36 | **RESHAPE → `spawn`** (into a kild) |
| `kild/room/post-message-tool.ts` | 50 | **RESHAPE → `send`** (to inboxes) |

### Reshaped (survive, lose the room skin)

| File | Lines | What changes |
|---|---|---|
| `kild/room/room-manager.ts` | **809 → ~250** | Delete: `nudgeIfIdleWithoutReport` (idle/posted failsafe → PRP), `notifyOpener`/opener notifications, the lead concept (`participants[0]`), decision-blocked close, `handleCloseRoom` lead check, `postFromHuman`/`postAs`/`HUMAN` split, `halt`, `recordMemory` synthesis spawn. Keep: open (spawn a session per agent), the session-bus subscription (model/cost/pi-session capture), `join`/`drain` (the honryo inbox), directed post → route. **RESHAPE → `KildManager`** |
| `kild/room/room-types.ts` | **333 → ~180** | Delete: `HUMAN`, `RoomLifecycleState`, `decisions`, `implicit`/`system` on messages, lead semantics, `RoomDecision`. Rename `Room→Kild`, `RoomParticipant→Agent` (keep the `spawned/attached`→`owned/attached` axis), `RoomMessage→Message`. **RESHAPE** |
| `kild/room/room-registry.ts` | 184 | Sound state store + write-through persistence. Strip `decisions` from snapshots, drop lifecycle-state persistence detail, rename. **RESHAPE → `KildRegistry`** |
| `kild/room/attached.ts` (Mailbox) | 106 | The honryo pull transport — becomes the **universal inbox** (owned agents drain by push, attached by pull). Keep the wake-cap loop guard. **RESHAPE → the inbox** |
| `kild/room/claude-stop.ts` | 85 | The Claude-Code Stop-hook shaper — this is literally the honryo's turn-boundary drain. Keep; rename `room→kild` in the emitted guidance. **RESHAPE (keep)** |
| `kild/operator/engine-client.ts` | 167 | The CLI's HTTP client. Keep the client, retarget to the new `/api/kilds*` endpoints, drop `spawnSession(operator:true)` + `openRoom`/`closeRoom` room shapes → `spawn`/`send`/`attach`/`inbox`. Move out of `operator/`. **RESHAPE → `engine-client.ts`** |
| `kild/operator/rooms-status.ts` | 100 | The compact git-status + **cross-kild collision** computation — pure, valuable observability. Keep the collision/compaction logic, drop `openDecisions`. Move out of `operator/`. **RESHAPE → `kild-status.ts`** |
| `kild/mechanism-prompt.ts` | 90 | Trim the room-comms paragraph, the `needs-decision` paragraph, and the `close_room` paragraph → keep only the `spawn`/`send` transport facts + `<available-models>`. The "how to operate" philosophy (verify-before-believe, escalate) is borderline intelligence but AGENTS.md blesses one generic mechanism prompt — keep it minimal. **RESHAPE** |
| `kild/memory.ts` | 176 | Split. The engine-written run **LOG.md** (built from held state) is mechanism — keep a thin kild-native ledger. The **synthesis session** (spawns an LLM with a persona to curate `MEMORY.md`) is intelligence — **MOVE → PRP**. `memory.dir` stays as the generic config hook. **RESHAPE + partial MOVE** |
| `worker.ts` | **297 → ~200** | Delete: the `operatorEnabled` tool branch (open/post/status/close room tools), `KILD_ROOM_LEAD`/`isRoomLead`, `close_room` lead tool, the implicit-reply auto-post at `agent_end` (`postedThisTurn`/`turnText` narration). Reshape the two room tools → `spawn`/`send`. Keep: worktree ensure, model/auth via `ModelRuntime`, resource loader/skills, fork, first-turn preamble. **RESHAPE** |
| `server.ts` | **779 → ~550** | Delete the operator framing on `/api/sessions` (`operator:true`), the `room_halt` WS verb, `resolve*RoomActor` attribution wiring, the `postFromHuman`/`postAs` split. Rename `/api/rooms*` → `/api/kilds*`; add `/api/kilds/:id/agents(/attach)`, `/api/kilds/:id/messages`, `/api/kilds/:id/land`. Keep: worktrees, personas, transcripts, git-review, `/api/open(-url)`, the WS event bus. **RESHAPE** |

### Untouched mechanism (KEEP as-is)

`kild/worktree.ts` (299) · `kild/worktree-status.ts` (147) · `kild/git-review.ts` (401) ·
`kild/sessions.ts` (319, already room-agnostic — reshape only the `SessionCallbacks` names
`onMessage/onInvite/onCloseRoom` → `onSend/onSpawn`) · `kild/session-transcript.ts` (106) ·
`kild/events.ts` (62) · `kild/agents.ts` (101, persona discovery) · `kild/models.ts` (23) ·
`kild/skills-profile.ts` (18) · `kild/projects.ts` (53) · `kild/config.ts` (140, drop
`memory.synthesis` if synthesis moves).

---

## Consumers (our own — reshape in lockstep, no compat shim)

| Consumer | Lines | Verdict |
|---|---|---|
| `pi-extension/index.ts` | 727 | **The biggest reshape.** Today it makes a *pi* session "the operator": `operatorGuide()`, `<kild-operator>`, `kild_open_room`, `@human` reporting, `needs-decision`. Rebuild on `spawn`/`send`/`attach`/`inbox`; delete the operator guide and decision vocabulary. **Open question:** with Claude Code as the honryo driving the CLI + `hooks/claude-stop`, is the pi extension still needed, or does it collapse into "pi is just another attached harness"? Decide before rebuilding. **RESHAPE (or DIE)** |
| `hooks/claude-stop` | 40 | The attached-honryo integration — Claude Code draining its inbox at turn end. Keep; retarget `kild room drain` → `kild inbox`. **RESHAPE (keep)** |
| `cli.ts` | 941 | Delete the whole `operator` command group, `--participants`/`--to` room framing, the `room_halt`/`room_open`/`room_post` WS verbs. Rebuild the verb set: `open`/`spawn`/`attach`/`send`/`inbox`/`ls`/`show`/`land`/`stop` (see the design brief's CLI table). Keep `project`/`agent`/`worktree`/`run`. **RESHAPE (major)** |

---

## Tests — die with their modules

**DIE:** `room-router.test.ts` (188) · `room-decisions.test.ts` (130) · `room-events.test.ts`
(65) · `rest-room-attribution.test.ts` (99) · `flue/worktree-sandbox.test.ts` (48). ~530 lines.

**REWRITE (module reshaped):** `room-manager.test.ts` (938 — the giant; most of it tests
deleted behaviour: lead routing, idle nudge, decision-blocked close, operator notify) ·
`operator/rooms-status.test.ts` (244 — keep collision tests, drop decision tests) ·
`server.attached.test.ts` · `server.post-to.test.ts` · `server.room-cwd.test.ts` ·
`cli.attached.test.ts` (224) · `worker.fork.test.ts` (rename only).

**KEEP:** `worktree*.test.ts` · `worktree-status.test.ts` · `git-review.test.ts` ·
`events.test.ts` · `models.test.ts` · `config.test.ts` · `agents.test.ts` · `sessions.test.ts` ·
`session-transcript.test.ts` · `skills-profile.test.ts` · `attached.test.ts` (inbox) ·
`memory.test.ts` (trim synthesis) · `mechanism-prompt.test.ts` (trim).

> Rule from AGENTS.md holds: never delete a test to make a gate green. These die because their
> *module* dies — say so in the commit, one line each.

---

## Docs — the paper trail

| Doc | Verdict |
|---|---|
| `VISION.md` | **KEEP** — already speaks the model ("Fracture the Honryū", "the Tōryō — one director"). The vision was right; the implementation drifted into rooms. Light edit: Room→kild. |
| `HANDOVER.md` | **RESHAPE** — supersede the room-patching roadmap (slices 5/6 are subsumed by this teardown). |
| `README.md` · `engine/README.md` | **RESHAPE** — drop the "Flue layer" src-map lines; Room→kild. |
| `engine/COMPARISON.md` | **DIE** (or move to an `archive/`) — historical Flue-vs-Rust decision record; the Flue bet it defends is now being deleted. |
| `docs/attached-participants.md` | **RESHAPE (keep)** — this is the honryo transport spec; rename room→kild. |
| `docs/fleet-observability.md` · `docs/room-mailbox-notes.md` · `docs/pi-extension-plan.md` | **DIE** — trackers/review-notes for work now either done or being torn out. |
| `docs/upstream-worktree-sandbox.md` | **DIE** — tied to the Flue sandbox being deleted. |
| `docs/manual-smoke-skills-profile.md` | **RESHAPE** — retarget `kild operator` → new verbs. |

---

## Suggested order of operations

1. **Cut the Flue lane** (Demolition 1) — isolated, zero behaviour change, gets the tree honest. One commit.
2. **Rename the domain** — `Room→Kild`, `RoomParticipant→Agent`, `RoomMessage→Message`, the `spawned/attached`→`owned/attached` axis. Mechanical; keeps gates green.
3. **Delete addressing choreography** — `room-router.ts`, the lead concept, implicit replies, `@human`/`HUMAN`. Replace with directed `send`. This is where the recurring addressing bugs die.
4. **Collapse the tiers** — delete `operator/*` tools + the operator worker branch; `invite_agent`/`open_room` → one `spawn`; `post_message`/`post_room` → one `send`.
5. **Move intelligence out** — `room-decisions.ts`, the idle-nudge failsafe, memory synthesis → PRP.
6. **Reshape the surfaces** — `server.ts` routes, `cli.ts` verbs, `pi-extension` (or retire it), `hooks/claude-stop`.
7. **Add the new mechanism** — `/api/kilds/:id/land` + seat guardrails (push-protection, spend cap, hard-halt).

Each step is a green-gates checkpoint. Nothing here is a shim; every rename is a real move.
