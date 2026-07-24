# Room mailbox: structured `to`, the busy-race finding, and the pi-extension plan

Notes from the 2026-07-22 review of `engine/src/kild/room/`. Goal: dead simple,
testable, lego. Branch: `room-structured-to`.

## 1. What changed — structured `to` (kills a real bug)

**Before:** a participant's recipients were scraped from the message *text* with a
lookbehind/lookahead regex (`parse-mentions.ts`). Two proven failure modes:

- **False positives on ordinary code/CSS.** `@property`, `@Component`, `@Injectable`,
  `@media`, `@keyframes` all parsed as handles. An unknown handle made `post()` return
  `rejected` *and* spam a system warning — so a worker writing "@worker fix the
  @Component bug" got its `post_message` reported as failed even though `@worker` was
  delivered.
- **False negatives.** "worker please…" (no sigil) addressed nobody → silent
  non-delivery. Delivery depended on the model emitting exact sigil syntax in prose —
  the weakest possible contract, and the first thing to break on non-Claude models.

**After:** addressing is a structured `to: string[]` parameter on `post_message`.
`parse-mentions.ts` is deleted. The one addressing rule, in `RoomManager.post()`:

> a system notice targets no one · else an explicit `to` wins · else the post goes to
> the room lead (the orchestrator).

A typo'd handle is returned as a clean error to the *calling agent* (its tool result),
never recorded or turned into room spam, so the model can self-correct. The router was
already text-agnostic (`message.to` authoritative); this makes the *producer* of `to`
structured too. Net **−11 lines**, and robust across vendors.

Touched: `post-message-tool.ts` (+`to` param), `worker.ts` (passes `to`),
`room-manager.ts` (`post()` rule + `postFromHuman`/`postAs` gain optional `to`),
`room-router.ts` (dropped dead `hasNoDeliverableRecipients`), `server.ts`/`cli.ts`
(dropped `addressKickoff`/mention-scraping — the manager defaults kickoff to the lead),
and the six room agent profiles (`.pi/agents/*.md`) now instruct `to`, not `@name`.

## 2. Busy-race: does a prompt arriving mid-turn queue or race? → QUEUES. No watcher needed.

The question was whether eager-push delivery (`post()` → `sessions.prompt(stdin)`) races
when a participant is mid-turn. **Settled by code, not guesswork:** `worker.ts`
(lines ~165–185) pushes every prompt onto a `promptQueue` and drains it strictly
sequentially behind a `draining` guard:

```
const promptQueue = [];
let draining = false;
async function drainPrompts() {
  if (draining) return;            // a mid-turn arrival is enqueued, NOT started concurrently
  draining = true;
  while (promptQueue.length) { await session.prompt(promptQueue.shift().text); }
  draining = false;
}
```

A prompt that arrives during `session.prompt()` is enqueued and picked up by the running
loop after the current turn resolves. No interleave, no drop. **The watcher idea is
therefore unnecessary — building it would be wasted work.** (One minor property: N
messages arriving during one busy turn are then processed as N sequential turns, not
batched into one context. Fine for coordination; only revisit if small-turn token cost
ever bites. YAGNI.)

Confirmed live (see §3): the run completed a full delegate→verify→report→close cycle
with no lost or interleaved turns.

## 3. Live cross-vendor validation (gpt-5.6-sol + MiniMax-M3)

A throwaway 2-agent room, `orchestrator=openai-codex/gpt-5.6-sol`,
`worker=minimax/MiniMax-M3`, task "create hello.txt containing `kild`":

```
human        -> [orchestrator]        (kickoff defaulted to lead — no @mention)
orchestrator -> [worker]              delegated with a definition of done (structured to)
worker       -> [orchestrator]        did the work, reported back (structured to)
orchestrator -> [human]               DONE, byte-level verified, then closed the room
hello.txt == "kild"                   ✅ real work landed
```

This proves the previously-unproven leg: **gpt-5.6-sol can drive the orchestrator loop**
(delegate, verify, close) and use `to` correctly; MiniMax-M3 works as a worker. The
regex removal is validated end-to-end with real non-Claude models.

## 4. Tracing

Every post funnels through `RoomManager.post()` → one structured stderr line via
`traceRoomPost` (grep `room.post`). Programmatic tracers should instead
`roomManager.subscribe(...)` — every post is already broadcast as a `roomMessage`, so a
full ordered trace needs no new system. Swap `traceRoomPost`'s body for a real logger
later without touching call sites.

## 5. Deferred (on purpose)

- **Lifecycle collapse** (4 states + 6 guards → running|stopped + visibility flag).
  Kept off this branch to keep the `structured-to` change atomic/reviewable. Blast
  radius verified engine-internal: clients read `RoomSummary.stopped` (boolean),
  not the raw state string, so it can be done as its own change safely. Note the
  halted↔closed distinction is load-bearing (halt = stopped-but-visible; close =
  archive+remove), not pure cruft — collapse them via a flag, don't erase the behavior.
- **Orchestrator open/halt tools.** Structured `to` already lets the orchestrator (the
  lead) reliably address anyone; as lead it holds `invite_agent` + `close_room`. New
  in-room `open`/`halt` tools not added: `halt` is the human's safety valve, `open` is
  the brain tier (already has fleet tools). Revisit only with a concrete need.

## 6. pi extension — requirements to test later (do NOT build yet)

Target: the room primitive slots into **native pi as a skill/extension**, so pi can
drive kild's rooms without the kild engine running, AND kild keeps using the same core.
The seam already exists — `RoomDelivery` — so this is one core + two host adapters, not
a fork.

Extract the **coordination core** (pure, zero host coupling):
- `room-router.ts` (routing + implicit-reply rule + delivery policy)
- `room-types.ts`, `room-events.ts`, `room-lifecycle.ts`
- the addressing rule (system→none · explicit `to` · else lead) — currently in `post()`;
  lift it into the core so both hosts share it.

Keep **host-specific** (the adapter each host provides):
- transport: subprocess-per-participant + line-delimited JSON control lines (kild), vs
  pi's in-session agent-spawn + tool calls (pi extension).
- `RoomDelivery.{deliverAsTurn, broadcast}` — the injection point.
- persistence + WS broadcast (kild engine only).

Open questions to answer before building:
- Does native pi expose an agent-spawn primitive an extension can call to create a
  "participant"? (kild built rooms *because* pi's coms-net POC scraped prose and failed —
  confirm what pi natively offers now.)
- Can a pi extension register `post_message`/`invite_agent` as tools and receive the
  turn-delivery callback? If yes, the pi `RoomDelivery` adapter is thin.
- Does "pi drives kild" actually need the extension, or is kild's existing CLI/HTTP
  enough for a pi agent to drive rooms over Bash? Decide before extracting.

Only extract once there's a concrete "pure pi, no kild engine" use case; until then,
drive kild from pi over its existing CLI/HTTP.
