# Feature: Primitives Audit Slice 8a — Room Opener Lifecycle Events

## Summary
Add a room-owned `openedBy` session id and use the existing `sessionManager.prompt` control line to wake the fleet session that opened a room when it needs operator attention. The REST/fleet path carries the opener id, pure room-event helpers decide whether and how to notify, and `RoomManager` invokes them for participant `@human` posts, halts, and closes without changing cockpit WebSocket frames or creating a transport.

## User Story
As the fleet brain that opened a workstream room
I want to receive explicit lifecycle and gate prompts while my session is live
So that I can reconcile, decide, and update the run ledger without manual wakes or polling loops.

## Problem Statement
A fleet brain is event-blind between turns: its `open_room` request does not identify its session and room lifecycle/post events only reach the cockpit broadcast. Close-driven workstream progression therefore stalls until a human manually wakes the brain. The solution is testable when only a live opener that is not a room participant receives labeled prompts for a participant’s `@human` post, room halt, or room close, and no room post is created by that delivery.

## Solution Statement
Pass the spawning session’s id to workers as manager-owned `KILD_SESSION_ID`; have the fleet `open_room` tool read it and send `openedBy` in the existing `POST /api/rooms` JSON. Store the optional id on the live room. Add a pure `room-events.ts` module that (a) formats deterministic, clearly labeled operator-notification text and (b) returns an opener target only when `openedBy` exists and is not a participant session. `RoomManager` calls `sessions.prompt(target, text, 'kild')` directly after qualifying state/post events; the existing missing-session no-op makes a dead opener a silent drop. The brain prompt becomes event-driven, retaining `rooms_status` solely for cold-resume reconciliation.

## Metadata

| Field | Value |
| --- | --- |
| Type | ENHANCEMENT |
| Complexity | MEDIUM — session env, REST contract, room state, lifecycle ordering, and prompt guidance intersect |
| Systems Affected | `engine/src/kild/sessions.ts`, fleet REST client/tool, room domain/manager, `engine/src/server.ts`, brain prompt, Bun unit tests |
| Dependencies | Bun test runner (Bun runtime); TypeScript ^5.6.0; Hono ^4.8.3; `@earendil-works/pi-coding-agent` 0.80.7 |
| Estimated Tasks | 7 |

---

## Lifecycle (append-only)

- **Created:** 2026-07-17T11:42:03Z
- **Modified:** 2026-07-17T11:42:03Z; 2026-07-17T12:03:00Z; 2026-07-17T12:35:24Z
- **Commits:** `0d44720` — implementation
- **Agent / Session:** worker(openai-codex/gpt-5.2) / kild room session; worker(openai-codex/gpt-5.2) / kild room implementation session
- **Back refs:** `HANDOVER.md` — slice-8 requirements and live reproductions
- **Forward refs:** none

> **Append-only:** `Created` is set once; every other field is a list you only ever add to — never overwrite or remove existing entries. Keep references bidirectional when a related plan exists.

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║   ┌─────────────┐       ┌─────────────┐       ┌──────────────────────────┐    ║
║   │ Fleet brain │ ────► │ open_room   │ ────► │ Room events only reach   │    ║
║   │ session     │       │ POST /rooms │       │ cockpit / room log       │    ║
║   └─────────────┘       └─────────────┘       └──────────────────────────┘    ║
║   USER_FLOW: Brain launches a room; participant asks @human or closes; brain  ║
║   remains idle until a human manually wakes it or it polls.                    ║
║   PAIN_POINT: Close/gate progression is not prompt-driven and stalls.          ║
║   DATA_FLOW: REST opener identity is absent; RoomManager broadcasts only.     ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║   ┌─────────────┐       ┌─────────────┐       ┌──────────────────────────┐    ║
║   │ Fleet brain │ ────► │ POST /rooms │ ────► │ Room stores openedBy     │    ║
║   │ session id  │       │ + openedBy  │       │ and lifecycle/post log   │    ║
║   └─────────────┘       └─────────────┘       └──────────────────────────┘    ║
║                                      │                                        ║
║                                      ▼                                        ║
║                         ┌──────────────────────┐                              ║
║                         │ NEW_FEATURE: prompt  │ ◄── direct SessionManager    ║
║                         │ live nonparticipant  │     delivery; no room post   ║
║                         └──────────────────────┘                              ║
║   USER_FLOW: Brain receives close/gate/halt prompt, reconciles, acts, and      ║
║   atomically updates its ledger; `rooms_status` is used only after cold resume.║
║   VALUE_ADD: A live fleet run advances close-driven workstreams with zero       ║
║   manual wakes. DATA_FLOW: worker env → tool JSON → REST → Room → pure decision║
║   → sessionManager.prompt.                                                      ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|---|---|---|---|
| Fleet `open_room` | opener session is anonymous | sends `openedBy` from manager-issued env | room can wake its owner |
| `RoomManager` | participant `@human`, halt, close only broadcast/archive | direct labeled prompt to eligible opener | brain receives gate/lifecycle turn |
| `.pi/agents/brain.md` | polling after gate-prone stages | event contract plus cold-resume reconciliation | fewer stalled fleet runs |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|---|---|---:|---|
| P0 | `engine/src/kild/sessions.ts` | 75-90, 180-229 | Manager-owned worker env and silent missing-session `prompt` behavior |
| P0 | `engine/src/kild/room/room-manager.ts` | 112-149, 206-239, 312-377 | Open/post/halt/close ordering and the sole address-resolution point |
| P0 | `engine/src/server.ts` | 246-296 | Existing `POST /api/rooms` validation and room open flow |
| P1 | `engine/src/kild/room/room-router.ts` | 30-77 | Do not turn an operator notification into a routed room message |
| P1 | `engine/src/kild/room/room-types.ts` | 43-75 | Live Room and OpenRoomSpec contract |
| P2 | `engine/src/kild/room/room-router.test.ts` | 11-70 | Bun pure-function fixture/assertion style |
| P2 | `.pi/agents/brain.md` | 25-72 | Ledger and monitor/gate instructions being replaced |

**External Documentation:**

| Source | Section | Why Needed |
|---|---|---|
| [Bun Test Runner](https://bun.sh/docs/test#test-filtering) | Test Filtering | Confirms `bun test <path>` runs the focused pure-helper test; Bun discovers `*.test.ts`. |
| [Bun environment variables](https://bun.sh/guides/runtime/read-env) | Read environment variables | Confirms workers/tools read the manager-injected `process.env.KILD_SESSION_ID`. |
| [Hono docs](https://hono.dev/docs/#hono) | Hono / Web Standards | Existing Hono route stays the single JSON REST boundary; no new protocol or middleware. |

---

## Patterns to Mirror

**NAMING_CONVENTION:**

```typescript
// SOURCE: engine/src/kild/room/room-router.ts:13-16
export function formatDelivery(roomName: string, from: string, text: string): string {
  return `[#${roomName}] @${from}: ${text}`;
}
```

**ERROR_HANDLING:**

```typescript
// SOURCE: engine/src/kild/room/room-manager.ts:50-61
function fail<T>(
  code: 'not_found' | 'invalid_state' | 'rejected',
  message: string,
): CommandResult<T> {
  return { ok: false, code, message };
}
```

**LOGGING_PATTERN:**

```typescript
// SOURCE: engine/src/kild/sessions.ts:227-229
prompt(id: string, text: string, from?: string): void {
  this.sessions.get(id)?.session.prompt(text, from);
}
```

No logger exists on this path. Mirror this intentional no-op: a dead/missing opener is silently dropped rather than logged, retried, or treated as an error.

**REPOSITORY_PATTERN:**

```typescript
// SOURCE: engine/src/kild/room/room-registry.ts:20-28
create(room: Room): void {
  this.rooms.set(room.id, room);
}

get(roomId: string): Room | undefined {
  return this.rooms.get(roomId);
}
```

**SERVICE_PATTERN:**

```typescript
// SOURCE: engine/src/kild/room/room-manager.ts:350-366
const message: RoomMessage = {
  id: this.createId(),
  roomId,
  from,
  to: opts.system ? [] : (opts.to ?? parseMentions(text)),
  text,
  ts: Date.now(),
  implicit: opts.implicit,
  system: opts.system,
};
this.registry.appendMessage(roomId, message);
routeRoomMessage(room, message, this.delivery());
```

**TEST_STRUCTURE:**

```typescript
// SOURCE: engine/src/kild/room/room-router.test.ts:1-30
import { expect, test } from 'bun:test';

function message(from: string, to: string[], text: string): RoomMessage {
  return { id: 'm1', roomId: 'r1', from, to, text, ts: 0 };
}

test('delivers a mention to that participant as a turn AND broadcasts it', () => {
  // construct fixture, call pure helper, assert exact output
});
```

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `engine/src/kild/sessions.ts` | UPDATE | inject immutable spawned session id as `KILD_SESSION_ID` |
| `engine/src/kild/fleet/engine-client.ts` | UPDATE | add optional `openedBy` to existing REST request shape |
| `engine/src/kild/fleet/open-room-tool.ts` | UPDATE | supply `process.env.KILD_SESSION_ID` when the brain opens a room |
| `engine/src/kild/room/room-types.ts` | UPDATE | add optional opener ownership to open spec/live Room |
| `engine/src/kild/room/room-events.ts` | CREATE | pure lifecycle formatting and opener-target decision functions |
| `engine/src/kild/room/room-events.test.ts` | CREATE | focused Bun tests for format and target eligibility |
| `engine/src/kild/room/room-manager.ts` | UPDATE | retain opener and direct lifecycle/gate prompts through injected sessions |
| `engine/src/server.ts` | UPDATE | validate/forward optional REST `openedBy`; no WS contract change |
| `.pi/agents/brain.md` | UPDATE | replace polling guidance with lifecycle event contract |

---

## NOT Building (Scope Limits)

- No new WebSocket frame, event transport, callback endpoint, queue, polling timer, or cockpit change; notifications reuse `sessionManager.prompt` only.
- No delivery to an opener that is a room participant, no prompt-generated `RoomMessage`, and no replay/retry for dead sessions; cold-resume reconciliation is the deliberate fallback.
- No persisted/replayed lifecycle notification stream or archive revival; archived rooms remain read-only history.
- No E2E automation in this slice. The E2E acceptance is the **next fleet run** completing close-driven progression with zero manual wakes.

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### `[x]` Task 1: UPDATE `engine/src/kild/sessions.ts`

- **ACTION**: Make the manager-issued session id available to each worker.
- **IMPLEMENT**: Change `PiSession` construction to receive the `spawn()` id and set `KILD_SESSION_ID: id` after spreading `req.env`, alongside the existing manager-owned `KILD_*` values. Keep it unavailable for caller override.
- **MIRROR**: `engine/src/kild/sessions.ts:75-91` — manager selects worker env values after opaque request env; `engine/src/kild/sessions.ts:180-224` — spawn owns the id and constructs `PiSession`.
- **GOTCHA**: Do not put `KILD_SESSION_ID` in `SpawnRequest.env`; an untrusted WS caller can supply `env`, whereas the spawned id is the identity required by opener targeting.
- **VALIDATE**: `cd engine && bun run typecheck`

### `[x]` Task 2: UPDATE fleet open request and tool

- **ACTION**: Carry the current fleet session identity in the existing REST body.
- **IMPLEMENT**: Add `openedBy?: string` to `OpenRoomRequest`; in `createOpenRoomTool.execute`, construct the current request with `from: 'brain'` and `openedBy: process.env.KILD_SESSION_ID` only when nonempty, preserving REST callers that omit it.
- **MIRROR**: `engine/src/kild/fleet/engine-client.ts:5-14,36-41` — typed request serialized wholesale; `engine/src/kild/fleet/open-room-tool.ts:30-38` — tool-level sender attribution before `openRoom`.
- **GOTCHA**: Do not add `openedBy` to the tool’s TypeBox parameters: it is process identity, not model-controlled input.
- **VALIDATE**: `cd engine && bun run typecheck && bun run lint`

### `[x]` Task 3: UPDATE `engine/src/server.ts` and `engine/src/kild/room/room-types.ts`

- **ACTION**: Validate and retain optional opener ownership at the REST/domain boundary.
- **IMPLEMENT**: Add `openedBy?: string` to `OpenRoomSpec` and `Room`; declare it as `unknown` in the `POST /api/rooms` body, reject a non-string supplied value with 400, and pass an omitted/optional string into `roomManager.open`. Initialize `Room.openedBy` from the spec. Do not alter `ClientMessage`, room WebSocket cases, or cockpit protocol.
- **MIRROR**: `engine/src/server.ts:246-296` — unknown JSON field validation before constructing the spec; `engine/src/kild/room/room-types.ts:47-75` and `engine/src/kild/room/room-manager.ts:120-129` — Room/OpenRoomSpec and live construction.
- **GOTCHA**: REST callers without a session must remain valid and get no notification target; use optional fields rather than an empty-string sentinel in domain state.
- **VALIDATE**: `cd engine && bun run typecheck && bun run lint`

### `[x]` Task 4: CREATE `engine/src/kild/room/room-events.ts`

- **ACTION**: Isolate pure opener eligibility and operator-notification formatting.
- **IMPLEMENT**: Export a formatter that produces clearly labeled operator prompts for three event variants: participant post addressed to `@human` (room name, sender, post text), halt (room name and final non-system post), and close/archive (room name and final non-system post). Export an eligibility function that returns `room.openedBy` only when it exists and is absent from `room.participants`; otherwise `undefined`. Define “final post” by scanning the room log from the end for `!message.system`, with an explicit stable fallback when none exists.
- **MIRROR**: `engine/src/kild/room/room-router.ts:13-16` — pure named `format…` helper and stable room framing; `engine/src/kild/room/room-router.ts:20-24` — pure participant-set decision; `engine/src/kild/room/room-types.ts:21-40` — message/system flags.
- **PATTERN**: Keep pure helpers free of `RoomManager`, session runtime, clock, UUID, and transport imports so they can be exact-output unit tested.
- **GOTCHA**: The notification must be only a prompt string; never call `post`, `routeRoomMessage`, or emit `RoomOutbound`, or it could trigger self-repost/loop behavior.
- **VALIDATE**: `cd engine && bun run typecheck`

### `[x]` Task 5: CREATE `engine/src/kild/room/room-events.test.ts`

- **ACTION**: Add existing-style unit coverage for the pure decisions.
- **IMPLEMENT**: Use `bun:test` fixtures with room participants/session ids and messages to assert exact event strings for human-addressed participant post, halt, and close (including the last non-system message selection); assert targeting for no opener, eligible nonparticipant opener, and opener matching a participant session id. Include a room with only system messages to lock the fallback text.
- **MIRROR**: `engine/src/kild/room/room-router.test.ts:11-40,55-70` — local room/message fixtures and exact `expect(...).toEqual/toBe`; `engine/src/kild/fleet/rooms-status.test.ts:4-55` — pure projection tests without engine subprocesses.
- **GOTCHA**: Do not unit-test `SessionManager` subprocess behavior or REST here; this slice explicitly asks for pure decision pieces and a missing target is already intentionally a no-op at `sessions.ts:227-229`.
- **VALIDATE**: `cd engine && bun test src/kild/room/room-events.test.ts`

### `[x]` Task 6: UPDATE `engine/src/kild/room/room-manager.ts`

- **ACTION**: Deliver qualifying events directly to a live eligible opener.
- **IMPLEMENT**: Add one private helper that obtains the pure opener target and calls `this.sessions.prompt(target, formattedText, 'kild')`. Invoke it after recording/routing a participant-originated post whose resolved `message.to` includes `HUMAN`; invoke it after halt’s system notice is appended; invoke it during close after transition/archive preparation while the original live `room` and its log are still available. Use the pure helper’s final non-system lookup so participant-close’s preceding system notice does not replace the final meaningful post. Do not invoke on messages from `human`/`brain`, system notices, implicit replies, or any room whose opener participates.
- **MIRROR**: `engine/src/kild/room/room-manager.ts:206-239` — halt transition + notice sequencing; `engine/src/kild/room/room-manager.ts:312-336` — participant close adds a system notice then delegates close; `engine/src/kild/room/room-manager.ts:339-377` — one authoritative recorded message with resolved `to`; `engine/src/kild/sessions.ts:227-229` — silent dead-session drop.
- **GOTCHA**: Preserve normal stop/archive/broadcast behavior and never route the notification via `this.post`; direct `sessions.prompt` ensures it cannot become a room post or cause prompt/repost loops.
- **VALIDATE**: `cd engine && bun test src/kild/room/room-events.test.ts src/kild/room/room-manager.test.ts src/kild/room/room-router.test.ts`

### `[x]` Task 7: UPDATE `.pi/agents/brain.md`

- **ACTION**: Replace monitor polling guidance with the new event contract.
- **IMPLEMENT**: State that for every room the brain opens it is prompted on close/archive, participant `@human` gate posts, and halt; on every event it must reconcile relevant room/ledger state, act or surface a human decision, then atomically ledger-update. Retain startup/cold-resume `rooms_status` reconciliation, but explicitly forbid poll-after-gate-prone-stage loops as the normal liveness mechanism. Keep verify-before-believe and existing ledger ownership rules.
- **MIRROR**: `.pi/agents/brain.md:25-42` — imperative ledger rules; `.pi/agents/brain.md:61-72` — concise Monitor/steer/gate bullet format.
- **GOTCHA**: Do not claim delivery survives a dead brain session; dead-openers are intentionally dropped and reconciliation is the recovery path.
- **VALIDATE**: `rg -n "prompted on|cold-resume|rooms_status|poll" .pi/agents/brain.md && cd engine && bun run lint`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|---|---|---|
| `engine/src/kild/room/room-events.test.ts` | exact post/halt/close prompt formatting; final non-system lookup/fallback | clearly labeled actionable operator prompts |
| `engine/src/kild/room/room-events.test.ts` | missing opener, eligible opener, participant-opener exclusion | no loops and correct target decision |
| `engine/src/kild/room/room-manager.test.ts` (only if needed for integration seam) | injected `prompt` receives qualifying formatted text | manager calls existing session substrate without routing notification |

### Edge Cases Checklist

- [ ] REST caller omits `openedBy` and room behavior remains unchanged.
- [ ] REST caller sends non-string `openedBy` and receives 400.
- [ ] `KILD_SESSION_ID` cannot be overridden through `SpawnRequest.env`.
- [ ] opener session is no longer live: `sessionManager.prompt` silently no-ops.
- [ ] opener is one of the room participant sessions: no prompt.
- [ ] participant’s explicit/parsed recipients include `@human`: prompt includes participant and original text.
- [ ] system/implicit/nonparticipant posts do not prompt opener.
- [ ] halt/close report the final non-system post, skipping system halt/close notices.
- [ ] notification never enters `Room.log`, `routeRoomMessage`, or cockpit WS stream.

---

## Validation Commands

🔁 **Validation loop:** run setup before checks because kild worktrees do not share `node_modules`; fix and rerun any failing command to exit 0.

### Level 1: STATIC_ANALYSIS

```bash
cd engine && bun install && bun run typecheck && bun run lint
```

**EXPECT**: Exit 0; TypeScript and Biome pass.

### Level 2: UNIT_TESTS

```bash
cd engine && bun test src/kild/room/room-events.test.ts src/kild/room/room-manager.test.ts src/kild/room/room-router.test.ts
```

**EXPECT**: Focused pure-helper and room-manager regression tests pass.

### Level 3: FULL_SUITE

```bash
cd engine && bun test && bun run typecheck && bun run lint
```

**EXPECT**: Entire engine test suite, typecheck, and lint exit 0. No separate build script exists in `engine/package.json`.

### Level 4: DATABASE_VALIDATION

Not applicable — no database/schema change.

### Level 5: BROWSER_VALIDATION

Not applicable — cockpit WebSocket protocol and UI are explicitly unchanged.

### Level 6: MANUAL_VALIDATION

1. Start the engine and a live fleet brain session.
2. Open a room through the brain’s `open_room`; confirm its REST request carries the brain session id as `openedBy`.
3. From a non-opener participant, post an explicit `@human` gate question; verify the brain receives exactly one labeled prompt and its prompt does not appear as a room message.
4. Halt a room and close a room (including a participant-led close); verify each yields one labeled opener prompt containing room name plus last non-system post.
5. Repeat with an opener that is also a participant and with a stopped opener session; verify no prompt/retry/error occurs.
6. **NEXT FLEET RUN E2E acceptance:** complete a close-driven multi-room progression with zero manual wakes; ledger records each prompted event and reconciliation is used only for cold resume.

---

## Acceptance Criteria

- [ ] Spawned workers receive manager-owned `KILD_SESSION_ID` equal to their session id.
- [ ] Fleet `open_room` sends that value as optional `openedBy`; non-fleet REST callers can omit it.
- [ ] `Room` stores optional opener ownership and REST validates its type.
- [ ] A live nonparticipant opener receives clearly labeled direct prompts for close/archive, halt, and participant `@human` posts.
- [ ] Close/halt prompt includes room name and final non-system post; gate prompt includes participant name and post text.
- [ ] Dead or participant openers are silently not notified; notifications never re-enter rooms.
- [ ] Focused pure event-formatting/targeting tests and all engine checks pass.
- [ ] The next fleet run demonstrates zero-manual-wake close-driven progression.

---

## Completion Checklist

- [x] `cd engine && bun install` completed before validation.
- [x] All tasks completed in dependency order.
- [x] Level 1 static analysis passes.
- [x] Level 2 unit tests pass.
- [x] Level 3 full engine suite passes.
- [x] Levels 4-5 correctly omitted as not applicable.
- [ ] Manual next-fleet-run acceptance recorded in its ledger.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Notification prompts re-enter room routing and cause loops | MED | HIGH | Pure helpers return only a string/target; manager calls `sessions.prompt` directly, never `post`/router. |
| Opener is stale or dead | HIGH | LOW | Preserve `SessionManager.prompt` optional lookup/no-op; rely on cold-resume reconciliation, not retries. |
| Brain opens a room from an untracked/non-session REST caller | MED | LOW | `openedBy` is optional and validation permits omission; event eligibility returns no target. |
| System close/halt notices hide meaningful final work result | MED | MED | Pure helper scans backward for the final non-system log entry and unit tests that ordering. |
| Accidental cockpit protocol expansion | LOW | MED | Restrict REST change to existing POST body; leave `ClientMessage` and `RoomOutbound` unchanged. |

---

## Questionables

<details>
<summary>What should the stable fallback say when a halted/closed room has no non-system post?</summary>

Resolved by binding decision: `NO_FINAL_POST` is exactly `(no non-system posts recorded)`, asserted as exact output in `engine/src/kild/room/room-events.test.ts`.

</details>

---

## Agent Notes

**Approach chosen:** A room-owned optional opener session id with direct, best-effort prompt delivery. This fits the existing session substrate: `SessionManager.prompt()` already silently ignores a missing id (`engine/src/kild/sessions.ts:227-229`), while room messages are deliberately routed/broadcast separately (`engine/src/kild/room/room-router.ts:57-77`).

**Alternatives rejected:**

- New WebSocket event/cockpit subscription: violates the no-new-transport constraint and would still not wake an idle brain process.
- Persistent/replayed notification queue: overbuilds an intentional live-only signal; ledger plus `rooms_status` reconciles cold resumes.
- Delivering to participant-openers: risks self-directed turns/reposts; pure target exclusion is the required loop guard.
- Parsing prose to decide gate targeting: rejected because resolved `message.to` is the existing authority (`engine/src/kild/room/room-manager.ts:354-357`).

**Discovery table:**

| Category | File:Lines | Pattern Description | Evidence |
|---|---|---|---|
| Session substrate | `engine/src/kild/sessions.ts:75-91, 227-229` | manager overrides worker env; prompt is missing-id no-op | `env: {...req.env, KILD_ROLE...}` / optional map lookup |
| REST boundary | `engine/src/server.ts:246-296` | unknown JSON validated then converted to `OpenRoomSpec` | existing `cwd/project/worktree` checks |
| Lifecycle | `engine/src/kild/room/room-manager.ts:206-239, 312-336` | transition plus system notice/teardown | halt and participant close flows |
| Address authority | `engine/src/kild/room/room-manager.ts:350-377` | manager records resolved `to` once | `opts.to ?? parseMentions(text)` |
| No-loop delivery | `engine/src/kild/room/room-router.ts:43-60` | system/implicit broadcast but never turns | early return after broadcast |
| Tests | `engine/src/kild/room/room-router.test.ts:11-70` | local fixtures + exact Bun assertions | `expect(...).toEqual` |

**Confidence:** 10/10. The exact no-non-system fallback was resolved as `NO_FINAL_POST` and covered by an exact-output unit test.

---

## Amendments

<details>
<summary>2026-07-17T11:42:03Z — Initial plan</summary>

Plan-only artifact for primitives-audit slice 8a. No implementation changes included.

</details>

<details>
<summary>2026-07-17T12:03:00Z — Implemented opener lifecycle prompts</summary>

Completed tasks 1–7 in `0d44720`: manager-owned session identity, existing REST opener propagation, pure event/target helpers, direct non-reentrant prompt delivery, focused regressions, and brain event-contract guidance. The next-fleet-run E2E acceptance remains pending by design.

</details>
