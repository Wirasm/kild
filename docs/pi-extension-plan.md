# Running kild from the pi CLI — extension plan

Goal: drive kild's concurrent multi-agent rooms from the `pi` CLI, so a pi session can
open/steer/observe rooms the same way the kild CLI does today.

Research basis (pi 0.81.1, 2026-07-23): pi's own canonical multi-agent pattern is
**subprocess-per-agent** (the official `subagent` example spawns `pi --mode json`; `pi --mode
rpc` is the documented concurrency story), which is exactly kild's architecture. Pi
intentionally omits sub-agents/MCP/plan from core — extensions are the intended way to add
this. Extension API surface: `export default (pi: ExtensionAPI) => {}` with
`pi.registerTool` (typebox params) / `pi.registerCommand` / `pi.on('before_agent_start' →
{systemPrompt} | session_start | …)`. Auto-discovered from `~/.pi/agent/extensions/*.ts` or
`*/index.ts`. **API is pre-1.0, minor-version-breaking** — pin the version, and prefer the
RPC *wire protocol* over the churning SDK types for anything deep.

---

## There is no "Phase 2". The thin extension IS the architecture.

An earlier draft proposed a "Phase 2": a self-contained extension that reimplements kild's
orchestration inside pi and spawns `pi --mode rpc` members. **Dropped — it's redundant.**
kild's workers already run real pi sessions (`createAgentSession` from
`@earendil-works/pi-coding-agent`), so "members become real pi sessions" is already true.
Rebuilding the transport on pi RPC would re-derive everything kild's worker already does
(control-line protocol, mechanism-prompt injection, skills profiles, base-branch worktrees,
idle failsafe) for no gain — only to avoid running kild's engine, which is the product, not a
cost (the extension can auto-start it). Classic YAGNI: no current caller.

The only scenario that would ever revive a self-contained build: distributing kild's room
capability into a locked-down *pi-only* environment where a second process is genuinely
impossible — and even then, "ship the engine alongside the extension" beats "reimplement kild
inside pi." The `RoomDelivery` seam already exists if that hypothetical ever becomes real;
that's free latent optionality, not something to build now.

## The extension — thin pi surface over kild's existing engine

The extension is a **REST client**: it registers pi tools that call kild's existing HTTP API
(`engine/src/kild/fleet/engine-client.ts` already wraps every endpoint). kild's engine keeps
spawning its concurrent workers exactly as today — the extension adds zero orchestration
logic, just a pi-facing surface. Requires the kild engine running — the extension should
preflight (`GET /api/health`; if down, spawn `bun run serve` and wait for ready) so the pi
user never has to think about it.

### File layout
```
~/.pi/agent/extensions/kild/           (or a git/npm package with {"extensions":["./index.ts"]})
  index.ts        — default export: registers tools + prompt injection + status
  tools.ts        — the pi tools (below), calling kild's REST API
  client.ts       — tiny fetch wrapper over KILD_ENGINE (default http://localhost:4517)
  prompt.ts       — pulls kild's mechanism + models prompt (or inlines a room-usage guide)
  package.json    — pins @earendil-works/pi-coding-agent to the target version
```

### Tools to register (1:1 with kild's REST surface)
| pi tool | kild endpoint (engine-client fn) | purpose |
|---|---|---|
| `kild_open_room` | `POST /api/rooms` (`openRoom`) | open a room: name, project/cwd, participants[{name,agent,model}], base, worktree, kickoff → returns room id |
| `kild_rooms` | `GET /api/rooms/live` (`getLiveRooms` + `compactLiveRooms`) | list live rooms with git/collision + per-agent models — the observability glance |
| `kild_room_log` | `GET /api/rooms/live` (filter one) | full thread of one room (pull view) |
| `kild_room_post` | `POST /api/rooms/:id/post` (`postRoom`) | steer a room / assign work |
| `kild_room_close` | `POST /api/rooms/:id/close` (`closeRoom`) | close a room — ONLY on explicit human instruction (mirror the never-auto-close rule) |
| `kild_fleet_start` (opt) | `POST /api/sessions` (`spawnSession`) | spawn a detached fleet driver |

Params: typebox (`Type.Object`), not zod. `execute()` returns `{content:[{type:'text',...}],
details:{...}}`. Self-truncate large payloads (~50KB pi convention) — the compact `kild_rooms`
already trims to counts, so this mostly matters for `kild_room_log`.

### Prompt injection
`pi.on('before_agent_start', e => ({ systemPrompt: e.systemPrompt + '\n\n' + roomGuide }))`
where `roomGuide` teaches: when to open a room vs work solo, the tool names, and that a pi
session driving kild is the *operator* (so IT decides when to close). The model catalog
(`.kild/config.json` `models`) is injected engine-side already for room participants; the pi
driver could optionally surface it too.

### Effort / risk
~a few hundred lines, no new orchestration, reuses the tested engine + concurrency. Main risk
is just pinning the pi API version. This is the whole job — ship it and "pi drives kild" works.

---

## Templates to copy
- `pi-mcp-adapter` (npm) — the package manifest shape (`{"extensions":["./index.ts"]}`) and the
  "one tool proxying many" pattern for keeping the context window small.
- pi's official `subagent` example — only as a reference for the pi tool/extension idioms; we
  are NOT copying its orchestration (kild already does that, concurrently, in the engine).

## Non-goals / notes
- Don't reimplement kild's orchestration in the extension — that's the engine's job. The
  extension is a pi-facing REST client, nothing more.
- The older `~/.pi/agent/extensions/agent-rooms/` is an in-process/sequential room extension —
  the weaker pattern; not the model to follow (it loses concurrency). Now disabled (moved to
  `~/.pi/agent/extensions-disabled/`).
- "coms-net" is our internal artifact, absent from pi — the lesson (structured events, never
  prose-scraping) is already baked into kild and matches pi's own subagent example.
