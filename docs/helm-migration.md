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

> **These two tables are the RENAME only — stage one of two.** They show where each old route
> went the moment `3902409` landed, and several of them were deleted by the reshape that
> followed. Rows struck through below no longer exist. Port against **"The reshape — the final
> surface"** further down; this stage is kept because it is what the pin boundary crosses, not
> because it is the surface you build on.

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
| `GET /api/sessions` | `GET /api/agents` — **survives** (the inventory of processes on no roster) |
| `POST /api/sessions` | ~~`POST /api/agents`~~ — **deleted**; spawn into a kild |
| `POST /api/sessions/:id/prompt` | ~~`POST /api/agents/:id/prompt`~~ — **deleted**; prompting is a message |
| `POST /api/sessions/:id/stop` | ~~`POST /api/agents/:id/stop`~~ — **deleted**; `DELETE /api/kilds/:id/agents/:handle` |
| `GET /api/sessions/:id/transcript` | ~~`GET /api/agents/:id/transcript`~~ — **deleted**; the kild-scoped route is the only one |

### Unchanged by the rename

`GET /api/health` · `GET /api/personas` · `POST /api/open` · `POST /api/open-url`

~~`GET|POST /api/projects`~~ were unchanged by the rename and are **deleted** by the reshape —
nothing called them (the CLI writes `$KILD_HOME/projects.json` directly, the extension passes a
registered name in a body, helm confirmed zero references). The registry still scopes the
unscoped `GET /api/kilds`; it simply has no REST surface. See `api-surface.md` §9.

**`GET|DELETE /api/worktrees` and `POST /api/worktrees/prune` were unchanged by the rename and
are DELETED by the reshape.** The argument for keeping them was that a worktree outlives the
kild that created it, so the disk view needs its own family — but the reshape made
`GET /api/kilds` enumerate `kild/*` trees from git, which gives a record-less tree an id and
therefore an address. With that, one collection answers both lifetimes and the second family
is duplication. See "Worktrees folded in" below.

### Known duplication — resolved, and the other way round

`GET /api/agents/:id/transcript` and `GET /api/kilds/:id/agents/:handle/transcript` were two
routes to the same resource — one keyed by machine `id`, one by kild-scoped `handle`. That was
the old participant-vs-session split surviving the rename in new clothes, and it is the exact
shape that let "who is this addressed to?" hide for so long.

**Resolution: `GET /api/kilds/:id/agents/:handle/transcript` WINS, and is the only transcript
route.** `/api/agents/:id/transcript` is deleted.

An earlier draft of this section said the opposite — that `handle` is an addressing concept
and `id` should be the routing key, so the kild-scoped route would be removed. Build against
the kild-scoped one. Reasoning for the reversal is in `docs/api-surface.md` §1: two identifier
schemes for one object is the shape being deleted, and a handle is unique for a kild's
lifetime (a stopped agent stays on the roster, so a handle never rebinds), which makes it a
perfectly good key. The session id is also not the "pi implementation detail" that argument
assumed — it is a `randomUUID()` the manager assigns; `piSessionId` is the separate pi-level
handle.

## Payload shapes

### Agent (was participant)

```diff
- { name, kind: 'spawned'|'attached', persona, model, piSessionId, piSessionFile, idle, posted, tokens, cost }
+ { handle, ownership: 'owned'|'attached', persona, model, invitedBy?, piSessionId, piSessionFile, idle, tokens, cost }
```

- `name` → **`handle`** (the `@name` you address)
- `kind` → **`ownership`**, values `spawned`→`owned`, `attached` unchanged. **Always present on
  the wire — do not write `?? 'owned'`.** `OwnedAgent.ownership` is optional in
  `kild-types.ts`, but that is the STORED shape; every wire payload goes through
  `agentIdentity()`/`agentView()`, which resolve it. (One payload used to escape that: an
  archive loaded from a file written before the field existed went to the route untouched.
  It is now resolved on decode, like `seq`.)
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

### Archived kild

```diff
- { id, name, worktree, agents[], log[], cwd, base, landedSha }
+ { id, name, worktree, agents[],        cwd, base, landedSha, landed?, endedAt? }
```

**`log` is gone from `GET /api/kilds/archive` and from the WS `{archivedKild}` frame.** One
rule, no exceptions: no listing and no broadcast carries a log. Read an archived kild's log the
same way you read a live one — `GET /api/kilds/:id/messages`, cursored by `seq`. The archive
only grows, so this was the most expensive payload in the engine.

`endedAt?: number` is new, and it is what makes the cheap archive usable: epoch millis for
the last moment the engine knew the kild was alive — for one that stopped normally, when it
stopped. **Sort history on this.** Dropping `log` dropped the only temporal signal with it
(the last message's `ts` had been carrying that job by accident), which would have meant one
`/messages` request per archived kild just to order a list.

Wall-clock deliberately: `ts` cannot be a cursor because it can go backwards, but "when did
this end" is a fact about the world and `seq` cannot answer it. Use it for display order,
never for paging.

Optional only because archives written before the field exist on disk. **There is no
fallback** — an old archive has no end time, and the engine will not infer one from its last
message. Sort the unknowns last; we are not shimming a shape we own both sides of.

`landed?: {commits, files}` is new: what the land carried, counted when it merged. Measuring
afterwards is measuring the wrong thing (`base..HEAD` is empty once the branch is in base).

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
- `POST /api/kilds/:id/inbox/drain` — `{name}` → `{handle}`; the response's `posts[]` is now
  `messages[]` (`post` is a dead word: you post to a board, you send to a person)
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

**The socket is a SUBSCRIPTION now — every kild mutation is a REST call.** `kild_new`,
`kild_send`, `kild_spawn` and `kild_stop` are **deleted**, and the old `room_*` frames with
them.

They could not answer: the frame was enqueued, a rejection was `console.warn`ed inside the
engine, and the caller was told nothing. `kild new --agents not-a-persona` printed a header
inviting you to type into a kild that never existed, then hung forever. Adding a rejection
frame would have meant a second request/response protocol beside the one that already works,
so the frames went instead.

| Was | Now |
|---|---|
| `room_open` → `kild_new` | `POST /api/kilds` |
| `room_post` → `kild_send` | `POST /api/kilds/:id/messages` |
| `room_add` → `kild_spawn` | `POST /api/kilds/:id/agents` |
| `room_close` → `kild_stop` | `POST /api/kilds/:id/stop` |
| `room_halt` → `kild_halt` | *(deleted with the lifecycle states)* |

`spawn`, `prompt`, and `stop` (the bare-agent session verbs, for one-shot `kild run`) keep
their names and stay on the socket — they have no kild to answer for.

Subscribe first, then create: open the socket, wait for `open`, and only then `POST`. The
create's response is your answer, and nothing said in the first instants is missed.

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
- [ ] Move every kild mutation off the socket onto REST (the `kild_*` frames are gone); keep the socket as a subscription
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
- [ ] Drop any `ownership ?? 'owned'` — it is always present now, archives included
- [ ] Stop reading `log` off `GET /api/kilds/archive` and the `{archivedKild}` frame — use `/messages`
- [ ] Sort the archive on `endedAt` (not on `log.last.ts`, which is gone); unknowns last
- [ ] Full-text search over archived messages needs the logs — it is not a listing concern; say so if you want an endpoint
- [ ] Drop `/api/projects` (deleted; the registry has no REST surface)
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
an agent id and an attached harness has none. Two attached agents were indistinguishable.

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
actor identity is engine-derived"*). The actor is resolved from the caller's `agentId` (renamed
from `sessionId` — `session` means pi's conversation and nothing else), or is
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
