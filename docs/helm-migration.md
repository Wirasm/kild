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
+ { handle, ownership: 'owned'|'attached', persona, model, piSessionId, piSessionFile, idle, tokens, cost }
```

- `name` → **`handle`** (the `@name` you address)
- `kind` → **`ownership`**, values `spawned`→`owned`, `attached` unchanged
- **`posted` is gone.** It backed a reporting norm that moved to PRP.
- **`idle` stays** — it is state, not a norm: an agent that finished a turn and is waiting.
  Still safe to render as an attention signal.

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

`UiEvent` payloads are unchanged — `text`, `tool_start`, `stats`, `model`, `pi_session`,
`agent_end`, `session_end`, `error` all keep their shapes.

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
- [ ] Un-pin

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
