# helm migration — the kild API rename

Everything a helm client needs to move from the room-era API to the kild-era API.

**This is the one breaking change.** It lands as a single commit so there is exactly one pin
boundary. Nothing before it changed an endpoint helm consumes; nothing after it should either.

## Pinning

kild and helm live in separate repos and cannot merge atomically, so:

1. **Pin helm** to the last kild commit *before* the rename commit.
2. Land the rename in kild.
3. Update helm against this document.
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
- `git` and `totals` are unchanged, including `collidesWith`.

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

## What is *not* changing (yet)

The lifecycle states (`opening|running|halted|closed`), the `HUMAN` handle, and the lead-default
routing all survive this commit unchanged — the rename is deliberately mechanical. They are
removed in the follow-up that replaces the router with directed `send`, and that change will get
its own migration note.
