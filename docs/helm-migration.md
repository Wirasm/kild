# helm migration — the kild API rename

Everything a helm client needs to move from the room-era API to the kild-era API.

**This is the one breaking change.** It lands as a single commit so there is exactly one pin
boundary. Nothing before it changed an endpoint helm consumes; nothing after it should either.

## Pinning

> **The un-pin point moved.** When this guide was written the rename was the only
> breaking change, so pinning across that one commit was enough. It no longer is: the
> router commit removes fields (`system` on messages, `state`/`stopped` on kilds) and
> requires `to` on every send, and the reshape in `docs/api-surface.md` changes routes
> again. **Stay pinned until the reshape lands, then port once** against the final
> surface. Porting to the intermediate states would mean doing the work three times.

kild and helm live in separate repos and cannot merge atomically, so:

1. **Pin helm** to the last kild commit *before* the rename commit.
2. Land the rename, the router commit, and the reshape in kild.
3. Update helm against this document (and `api-surface.md` for the reshaped routes).
4. **Un-pin.**

Dual-serving both route families was considered and rejected — it would carry the room API
through the entire rebuild, which is the thing being deleted.

## What changed conceptually

A **room** is now a **kild**: a git worktree and the agents working in it. A **participant** is
now an **agent** — the only inhabitant type there is. The engine no longer has ranks (no lead,
no operator), and the human is no longer a participant.

An agent sits on one axis the engine cares about: `ownership` is `owned` (kild runs the
process) or `attached` (an external harness kild addresses but never spawns). Its `persona` is
just a string the engine stores and never interprets.

Full rationale: `docs/VOCABULARY.md` and `docs/DEMOLITION.md`.

## REST

### Kilds (was rooms)

| Was | Now |
|---|---|
| `GET /api/rooms/live` | `GET /api/kilds` |
| `GET /api/rooms/archive` | `GET /api/kilds/archive` |
| `POST /api/rooms` | `POST /api/kilds` |
| `POST /api/rooms/:id/post` | `POST /api/kilds/:id/messages` |
| `POST /api/rooms/:id/join` | `POST /api/kilds/:id/agents/attach` |
| `POST /api/rooms/:id/drain` | `POST /api/kilds/:id/inbox/drain` |
| `POST /api/rooms/:id/close` | `POST /api/kilds/:id/stop` |
| `GET /api/rooms/:id/git/commits` | `GET /api/kilds/:id/git/commits` |
| `GET /api/rooms/:id/git/files` | `GET /api/kilds/:id/git/files` |
| `GET /api/rooms/:id/git/diff?path=` | `GET /api/kilds/:id/git/diff?path=` |
| `GET /api/rooms/:id/participants/:name/transcript` | `GET /api/kilds/:id/agents/:handle/transcript` |

### Agents (was sessions)

| Was | Now |
|---|---|
| `GET /api/sessions` | `GET /api/agents` |
| `POST /api/sessions` | `POST /api/agents` |
| `POST /api/sessions/:id/prompt` | `POST /api/agents/:id/prompt` |
| `POST /api/sessions/:id/stop` | `POST /api/agents/:id/stop` |
| `GET /api/sessions/:id/transcript` | `GET /api/agents/:id/transcript` |

### Unchanged

`GET /api/health` · `GET|POST /api/projects` · `GET /api/personas` ·
`GET|DELETE /api/worktrees` · `POST /api/worktrees/prune` · `POST /api/open` ·
`POST /api/open-url`

**Why `/api/worktrees` survives alongside `/api/kilds`,** given a kild *is* a worktree: a
worktree outlives the kild that created it. An archived kild's tree is still on disk, and
listing or pruning a tree whose kild is long gone has to work when there is no kild to ask
about. So `/api/kilds` is the **live** view (workstreams with agents in them) and
`/api/worktrees` is the **disk** view (what exists on the filesystem, reclaimable or not).
They are two lifetimes, not two names for one thing.

### Known duplication, to be resolved before you migrate

`GET /api/agents/:id/transcript` and `GET /api/kilds/:id/agents/:handle/transcript` are two
routes to the same resource — one keyed by machine `id`, one by kild-scoped `handle`. That is
the old participant-vs-session split surviving the rename in new clothes, and it is the exact
shape that let "who is this addressed to?" hide for so long.

**Resolution:** `handle` is an *addressing* concept — it names a recipient for `send`. It is not
a routing key. One canonical resource family keyed by `id` (`/api/agents/:id/...`) wins, and the
kild payload already carries the `id`→`handle` mapping in its `agents[]`. The kild-scoped
transcript route is removed in the follow-up commit; do not build against it.

## Payload shapes

### Agent (was participant)

```diff
- { name, kind: 'spawned'|'attached', persona, model, piSessionId, piSessionFile, idle, posted, tokens, cost }
+ { handle, ownership: 'owned'|'attached', persona, model, invitedBy?, piSessionId, piSessionFile, idle, tokens, cost }
```

- `name` → **`handle`** (the `@name` you address)
- `kind` → **`ownership`**, values `spawned`→`owned`, `attached` unchanged
- **`posted` is gone.** It backed a reporting norm that moved to PRP.
- **`idle` stays** — it is state, not a norm: an agent that finished a turn and is waiting.
  Still safe to render as an attention signal.
- **`invitedBy` is new** (on the cheap view too): the handle that spawned this agent, absent
  for the kild's initial roster. It is the spawn edge as ground truth rather than something
  to infer from the log — enough to draw who-delegated-to-whom, and the only thing that tells
  five agents on one persona apart by origin. A spawn through the API with no credential
  reads as `"human"`, exactly as its sends do.

### Message (was room message)

```diff
- { id, roomId, from, to[], text, ts, implicit?, system? }
+ { id, kildId,  from, to[], text, ts,            system? }
```

- `roomId` → **`kildId`**
- **`implicit` is gone.** Turn-final narration is no longer auto-posted, so every message in the
  log is now something an agent explicitly sent. If helm rendered a `[narration]` treatment,
  delete it — the case cannot occur.
- `from` and `to[]` are unchanged. `to` remains the authoritative recipient list; it is never
  parsed from message text.

### Kild (was room)

```diff
- { id, name, worktree, participants[], state, stopped?, log[], decisions?, cwd, base, git?, totals? }
+ { id, name, worktree, agents[],       state,            log[],            cwd, base, git?, totals? }
```

- `participants` → **`agents`**
- **`decisions` is gone.** The `needs-decision[key]` ledger was a protocol the engine parsed out
  of message prose; it moved to PRP. If helm rendered open decisions, remove it.
- **`stopped` is gone** — it was a derived mirror of `state === 'halted'`. Read `state`.
- `git` and `totals` are unchanged.

**`collidesWith` is not on the wire — and never was.** Cross-kild collision detection is
computed by `computeCollisions()` inside `compactLiveKilds()`, which is the **CLI's** compact
view. `GET /api/kilds` does not carry the field. What it *does* carry is `git.changedFiles`, the
full per-kild changed-file list, which is everything needed to derive collisions client-side.
Deriving them in helm from `git.changedFiles` is the correct approach, not a workaround.

### Request body changes

- `POST /api/kilds` — `participants: [{name, persona, model}]` → `agents: [{handle, persona, model}]`
- `POST /api/kilds/:id/agents/attach` — `{name}` → `{handle}`
- `POST /api/kilds/:id/inbox/drain` — `{name}` → `{handle}`
- `POST /api/kilds/:id/stop` — the `force` field is **gone** (it existed only to bury open
  decisions, which no longer exist)
- `POST /api/kilds/:id/agents` — takes an optional **`task`**, the new agent's first message.
  It is delivered as an ordinary directed message from whoever the engine resolves the caller
  to be, so it lands on the log at `seq n` and needs no separate `POST /messages`. Same
  convenience as `kickoff` on create, for the same reason: an agent nobody has said anything
  to just idles. There is **no** `invitedBy` field to send — presence of one is a `409`, and
  the spawner is derived from the credential (session id or Bearer token) like every other
  actor. That is not new strictness, it is the same rule `from` has always had; it matters
  more here because the spawner is now a *message sender*.

## WebSocket

Connect to `/ws` as before. Frame names changed.

### Server → client

| Was | Now |
|---|---|
| `{ roomMessage: Message }` | `{ message: Message }` |
| `{ rooms: RoomSummary[] }` | `{ kilds: KildSummary[] }` |
| `{ archivedRoom: ArchivedRoom }` | `{ archivedKild: ArchivedKild }` |
| `{ room, participant, event }` | `{ kild, agent, event }` |
| `{ session, event }` | `{ agent, event }` |
| `{ sessions: SessionInfo[] }` | `{ agents: AgentInfo[] }` |

### Client → server

| Was | Now |
|---|---|
| `room_open` | `kild_new` |
| `room_post` | `kild_send` |
| `room_add` | `kild_spawn` |
| `room_halt` | `kild_halt` |
| `room_close` | `kild_stop` |

`spawn`, `prompt`, and `stop` (the bare-agent verbs) keep their names.

`kild_spawn` also takes an optional `task` — `{ type, id, agent, task? }` — with the same
meaning as on the REST route. This transport carries no credential, so the task is sent from
`"human"`, exactly as `kild_send` is. Prefer `POST /api/kilds/:id/agents` when you care
whether the spawn worked: the frame is still fire-and-forget.

`UiEvent` payloads are unchanged — `text`, `tool_start`, `stats`, `model`, `pi_session`,
`agent_end`, `session_end`, `error` all keep their shapes.

## The reshape — the final surface

Landed after the router commit. This is what helm ports against.

### Listing split into cheap and costly

`GET /api/kilds` used to compute git status per kild — and once orphan trees began
enumerating, per orphan too — and returned every message log alongside. Measured on an
8-worktree fixture: **57 git invocations, ~110ms**. On a 116-worktree machine that
extrapolates to ~813 per call.

| Route | Carries |
|---|---|
| `GET /api/kilds` | **cheap.** `id`, `name`, `cwd`, `worktree?`, `base?`, `orphan?`, `agents[]` (`handle`, `ownership`, `persona`, `model`, `idle`, `stopped`). **No `git`, no `log`, no `totals`.** 1 git call total, at any worktree count |
| `GET /api/kilds/status` | **costly.** The above plus `git` (`branch`, `ahead`, `behind`, `dirty`, `uncommittedFiles`, `changedFiles[]`, `conflictsWithBase`), `totals`, per-agent `tokens`/`cost`/`piSession*`, `landedSha` |
| `GET /api/kilds/:id` | one kild with git, **no log** |
| `GET /api/kilds/:id/messages?since=<seq>` | the log, as its own resource |

**Poll them on different cadences.** That is the entire point — the sidebar can refresh
identity and attention constantly and ask for git only when it renders it.

`?state=live|orphan` works on both. **`?state=reclaimable` is `/status`-only** — it reads
`ahead`, so the cheap route returns `400` naming where to ask rather than silently paying.

### Messages have a monotonic `seq`

`ts` is `Date.now()` and can go backwards, so it never worked as a cursor. `seq` is strictly
increasing within a kild, assigned from the log's own tail so it survives reload, and rides
WS `{message}` frames — so a client can distinguish new from replay.

`?since=<seq>` is **exclusive**: pass the last seq you saw. Works on archived kilds too.
Archives written before `seq` existed decode by position.

**Logs are gone from every listing payload.** If helm read the thread off the kild list, it
now reads `/messages`.

### One address per agent

Deleted: `POST /api/agents`, `POST /api/agents/:id/prompt`, `POST /api/agents/:id/stop`,
`GET /api/agents/:id/transcript`.

- **`POST /api/kilds/:id/agents`** — spawn into a kild. New, and it *answers*: spawning was
  WS-only and fire-and-forget, so a caller could never learn it failed. Takes an optional
  `task` (the new agent's first message, sent from the derived spawner) and records the
  spawner as the agent's `invitedBy`.
- **`DELETE /api/kilds/:id/agents/:handle`** — stop one agent, kild keeps running. The agent
  stays on the roster marked `stopped`, so its transcript stays addressable.
- **`GET /api/kilds/:id/agents/:handle/transcript`** — the only transcript route.
- **Prompting an owned agent is sending it a message.** `POST /api/kilds/:id/messages` is
  the one delivery path.
- `GET /api/agents` **survives** — it is the inventory of live processes on no roster
  (one-shot `kild run`), which no kild can answer.

### Worktrees folded in

Deleted: `GET|DELETE /api/worktrees`, `POST /api/worktrees/prune`.

`GET /api/kilds` **enumerates `kild/*` from git**, not the registry — so trees stranded by
previous engine runs finally have an id. They appear as kilds with `orphan: true`, no
agents, no log, and **`id` = worktree name**. A worktree kild did not create is never listed.

**`DELETE /api/kilds/:id?force=true`** is the disposal verb, and the guard is **authored
commits, not a clean tree**. Commits in `base..HEAD` refuse; uncommitted files are discarded
and *listed* in the response. **The branch always survives** (`branchKept: true`), which is
what makes `force` safe. An unresolvable base refuses rather than guessing.

Prune is now a filter: `GET /api/kilds/status?state=reclaimable`, then delete what you mean.
Nothing is removed behind your back.

### Land is two verbs

- **`GET /api/kilds/:id/land`** — dry run. Reports `wouldMerge`, `commits`, `files`,
  `collides`. **Touches nothing** (verified: HEAD, refs and status identical after).
- **`POST /api/kilds/:id/land`** — merges toward base, returns the merge `sha`. Returns
  `409` *with the full result* when it did not merge, never a `200 merged:false`.

It merges in the project's main checkout and **refuses if that checkout is not on base or is
dirty**, rather than updating refs behind it. So land fails while you have another branch
out — deliberate, but it will surface in the UI.

## Checklist

- [ ] Pin to the pre-rename kild commit
- [ ] Swap `/api/rooms*` → `/api/kilds*` and `/api/sessions*` → `/api/agents*`
- [ ] `participant.name` → `agent.handle`; `kind` → `ownership` (`spawned`→`owned`)
- [ ] `message.roomId` → `kildId`
- [ ] Rename WS frame handlers
- [ ] Delete the `[narration]` / `implicit` rendering path
- [ ] Delete open-decisions rendering
- [ ] Delete any `posted` attention logic; keep `idle`
- [ ] Drop `force` from the stop call

**Reshape:**

- [ ] Poll `GET /api/kilds` (cheap) for identity/roster/attention; `GET /api/kilds/status` on a slower cadence for git and cost
- [ ] Stop reading logs from listings — use `GET /api/kilds/:id/messages`
- [ ] Adopt `seq` as the cursor; `?since=` is exclusive; drop any `ts` ordering
- [ ] `?state=reclaimable` only on `/status`
- [ ] Swap `/api/agents/:id/*` for `/api/kilds/:id/agents/:handle`; prompting becomes a message
- [ ] Replace `/api/worktrees` with `GET /api/kilds` + `DELETE /api/kilds/:id`
- [ ] Render `orphan: true` kilds (this is where the stranded trees appear)
- [ ] Add the land gate against `GET`/`POST .../land`, and handle the `409`-with-result case
- [ ] Handle land's refusal when the main checkout is off-base or dirty
- [ ] Derive `collidesWith` client-side from `status`'s `changedFiles[]` — it is not a server field
- [ ] Drop any `ownership ?? 'owned'` — it is always present now
- [ ] Spawn with `task` instead of spawn-then-post; stop sending `invitedBy` (it is a `409`)
- [ ] Render `agent.invitedBy` if you want the delegation tree — it is ground truth now
- [ ] Optional: keep the token from `attach` and send it as `Bearer`, so helm's own messages read as its handle rather than `human`
- [ ] Un-pin

### `ownership` is always present

It used to be omitted for owned agents (absent meant owned), which made
`agent.ownership === 'owned'` false for every owned agent. It is now always `'owned'` or
`'attached'` on the wire. If helm wrote `?? 'owned'`, that can go.

### Attach returns a credential, and it is optional

Every attached sender used to be attributed as `'human'`, because attribution came from a
kild session id and an attached harness has none. Two attached agents were indistinguishable.

`POST /api/kilds/:id/agents/attach` now also returns a **token**. Send it as
`Authorization: Bearer <token>` and the message is attributed to **that handle**:

```
seq 2  from: "honryo"   ← Bearer
seq 3  from: "claude"   ← Bearer
seq 4  from: "human"    ← no credential
```

Seq 2 and 3 both read `human` before.

- **Optional by design.** No header, a non-Bearer scheme, or a blank Bearer all behave
  exactly as before (`from: "human"`). helm needs no change unless it wants to be named.
- Scoped to one `(kild, handle)`. A token for another kild is a `409`, not a silent fallback.
- Re-attaching returns the **same** token, so a session-start hook cannot invalidate the
  token its own running session holds.
- In memory only; dies with the kild. Nothing to persist or refresh.

**This is attribution, not authorization.** No route requires a token, and seat enforcement
is deliberately unbuilt — on a loopback engine any local process can read the token, so
gating `stop`/`DELETE` would break the CLI in exchange for ceremony.

Also note `from` on a message is now the sender's **handle**, not its persona. Two agents
running the same persona are finally distinguishable.

## Attribution: `from` is already engine-derived

Worth stating because it reads like an open item and is not. `from` is **not** client-supplied
attribution — supplying it is a hard rejection (`rest-attribution.ts`: *"from is not allowed;
actor identity is engine-derived"*). The actor is resolved from the caller's `sessionId`, or is
the human when no session is given. The field survives in the request schema only so that a
client passing it gets a loud error instead of being silently ignored.

## The router commit — what it removes on top of the rename

The rename was deliberately mechanical, so the lifecycle states, the `HUMAN` handle and the
lead-default routing all survived it unchanged. The follow-up removes them. Since you are
pinned across both, here is the combined delta:

- **`to` is required on every send.** The engine never infers a recipient — no lead default,
  no `@human`-wakes-lead, no "one agent so it must mean them". An empty or missing `to` is a
  rejection. `POST /api/kilds` takes `kickoff: {to: [handle], text}`.
  *(The CLI may still resolve `--to` for you when a kild has exactly one agent — that is a
  client convenience, resolved before the call. The engine has no such rule.)*
- **`system` is gone from `Message`.** Engine notices are no longer log entries; roster
  changes come from the `{kilds}` broadcast and the event stream. `Message` is
  `{id, kildId, from, to[], text, ts}`.
- **`state` and `stopped` are gone from kilds.** The lifecycle state machine
  (`opening|running|halted|closed`) is deleted. Liveness is presence: a kild in
  `GET /api/kilds` is live, one in `/archive` is stopped.
- **`kild_halt` is gone from the WS frames.** `halt` and `stop` collapsed into `stop`.
- **`HUMAN` no longer exists.** A human-driven harness attaches and gets an ordinary handle
  like any other agent. There is no privileged participant.

If helm rendered system notices in the thread, or keyed off `state`/`stopped`, both need
replacing — the first with roster diffs from the kild list, the second with which collection
the kild appears in.
