# Feature: Primitives Audit Slice 5 — Engine-Derived Actor Identity

## Summary

Make room actor attribution engine-derived on the kild engine REST path. Fleet tools will send the manager-owned `KILD_SESSION_ID` with `post_room` and `close_room`, the existing room-open `openedBy` session id will become the source of kickoff attribution, and the server will resolve session id → agent name before calling `roomManager.postAs()` / `roomManager.close()`. Success mode is intentionally limited to exactly two cases: session-aware agent callers derive identity from a live engine session, and sessionless REST callers remain `human` for cockpit/CLI compatibility.

## User Story

As the kild engine that brokers room actions
I want actor identity to come from live session state instead of client-provided prose
So that room transcripts, close actions, and opener-originated kickoffs are honestly attributed and cannot silently fall back to the wrong speaker.

## Problem Statement

Today the fleet REST/tool path still lets callers choose `from` strings (`brain`, etc.) even though the engine already owns the real session identity. That leaves room attribution forgeable, keeps open/post/close on different identity semantics, and risks silent human fallback when a session-aware caller supplies bad identity. The slice is complete when `/api/rooms`, `/api/rooms/:id/post`, and `/api/rooms/:id/close` accept only the two in-scope modes — derived actor from a known session id, or no session id meaning `human` — and tests prove derivation plus typed rejections for mixed/unknown identity inputs.

## Solution Statement

Reuse the opener-events slice’s manager-owned `KILD_SESSION_ID` as the single actor token. Add a small engine-side attribution helper that resolves REST request identity from session state: `openedBy` for room-open kickoff attribution, `sessionId` for room post/close attribution. The helper will consult `SessionManager` for session metadata, derive the actor name from the live session’s configured agent, and return a typed room-style rejection when the session is unknown, missing actor metadata, or combined with legacy `from`. Fleet model-facing tools stop exposing `from`; they send session identity only. Server handlers keep ordinary sessionless REST as `human` and do not build any wider capability-assembly / worker-tool-modes refactor.

## Metadata

| Field | Value |
| --- | --- |
| Type | REFACTOR |
| Complexity | MEDIUM |
| Systems Affected | `engine/src/kild/fleet/*.ts`, `engine/src/kild/sessions.ts`, new room REST attribution helper/test, `engine/src/server.ts` |
| Dependencies | Bun runtime/test runner; TypeScript ^5.6.0; Hono ^4.8.3; `@earendil-works/pi-coding-agent` 0.80.7 |
| Estimated Tasks | 6 |

---

## Lifecycle (append-only)

- **Created:** 2026-07-17T13:55:52Z
- **Modified:** 2026-07-17T13:55:52Z
- **Commits:** none yet
- **Agent / Session:** planner(openai-codex/gpt-5.2) / kild room worker session
- **Back refs:** `HANDOVER.md` — slice-5 audit target and scope notes; `.claude/PRPs/plans/primitives-audit-slice-8a-room-opener-events.plan.md` — prior slice that introduced manager-owned `KILD_SESSION_ID` and `openedBy`
- **Forward refs:** none

> **Append-only:** `Created` is set once; every other field is a list you only ever add to — never overwrite or remove existing entries. Keep references bidirectional when a related plan exists.

---

## UX Design

### Before State

```text
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │ Fleet tool  │ ──────► │ REST body   │ ──────► │ Room action │            ║
║   │ / caller    │         │ may carry   │         │ trusts      │            ║
║   │             │         │ free-form   │         │ caller text │            ║
║   │             │         │ `from`      │         │ attribution │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW: Fleet brain opens/posts/closes rooms by sending names like      ║
║   `brain`; cockpit/CLI sessionless REST defaults to `human`.                 ║
║   PAIN_POINT: Actor identity is caller-authored prose, not engine-derived    ║
║   session truth; unknown session-aware callers can only be handled ad hoc.   ║
║   DATA_FLOW: tool schema -> engine-client JSON -> server `from` checks ->    ║
║   `roomManager.postAs()` / `postFromHuman()` / `close()`.                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```text
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │ Fleet tool  │ ──────► │ REST body   │ ──────► │ Server      │            ║
║   │ / caller    │         │ carries     │         │ resolves    │            ║
║   │             │         │ session id  │         │ session ->  │            ║
║   │             │         │ or nothing  │         │ actor name  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                   │                                           ║
║                                   ▼                                           ║
║                          ┌─────────────┐                                      ║
║                          │ NEW_FEATURE │  ◄── typed rejection for mixed or    ║
║                          │ attribution │      unknown identity inputs         ║
║                          └─────────────┘                                      ║
║                                                                               ║
║   USER_FLOW: Fleet room actions identify themselves by live session id;      ║
║   cockpit/CLI sessionless REST still lands as `human`.                        ║
║   VALUE_ADD: Transcripts and room closes are honest, deterministic, and      ║
║   cannot fall back to fake/human attribution on bad session identity.        ║
║   DATA_FLOW: manager-owned env -> tool/engine-client JSON -> pure server      ║
║   attribution helper -> session lookup -> roomManager action.                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User_Action | Impact |
| --- | --- | --- | --- | --- |
| `engine/src/kild/fleet/post-room-tool.ts` | model can supply optional `from` | tool sends only `text` + engine-owned session id | brain posts to room | actor cannot be forged by tool input |
| `engine/src/server.ts` `/api/rooms` | kickoff may use client-provided `from` | kickoff actor derives from `openedBy` session -> agent name | brain opens room | room transcript attributes kickoff honestly |
| `engine/src/server.ts` `/api/rooms/:id/post|close` | server trusts optional `from` or falls back to `human` | server chooses derived actor or sessionless `human`, rejects mixed/unknown identity | post or close room | bad session identity becomes explicit rejection, never human fallback |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
| --- | --- | ---: | --- |
| P0 | `engine/src/server.ts` | 74-110, 246-319 | Current request parsing helpers and the exact room REST handlers being changed |
| P0 | `engine/src/kild/sessions.ts` | 80-98, 187-240 | Manager-owned `KILD_SESSION_ID`, live session metadata storage, and missing-session behavior |
| P0 | `engine/src/kild/fleet/post-room-tool.ts` | 6-29 | Current model-facing `from` schema to remove |
| P0 | `engine/src/kild/fleet/open-room-tool.ts` | 6-45 | Existing `openedBy` propagation from slice 8a |
| P1 | `engine/src/kild/fleet/engine-client.ts` | 5-63 | REST request shapes and JSON bodies for open/post/close |
| P1 | `engine/src/kild/fleet/close-room-tool.ts` | 6-25 | Close tool currently lacks session-aware identity |
| P1 | `engine/src/kild/room/room-manager.ts` | 123-176, 335-385 | `postFromHuman`/`postAs` split and close path the server ultimately invokes |
| P2 | `engine/src/kild/sessions.test.ts` | all | Existing small pure/unit test style for session-layer helpers |
| P2 | `engine/src/kild/room/room-manager.test.ts` | 307-355 | Existing room attribution assertions and typed rejection style |

**External Documentation:**

| Source | Section | Why Needed |
| --- | --- | --- |
| [Bun docs](https://bun.sh/guides/runtime/read-env) | Read environment variables | Confirms worker/fleet tools should read manager-owned `process.env.KILD_SESSION_ID`, not model input |
| [Hono docs v4.x](https://hono.dev/docs/api/request) | `HonoRequest` / `c.req.json()` / `param()` | Matches the existing server route parsing approach instead of introducing a new validation layer |
| [TypeBox docs](https://github.com/sinclairzx81/typebox#properties) | object properties and optional fields | Relevant to removing `from` from model-facing tool parameter schemas while keeping JSON shape changes explicit |

---

## Patterns to Mirror

**NAMING_CONVENTION:**

```typescript
// SOURCE: engine/src/kild/room/room-events.ts:20-28
export function openerNotificationTarget(
  room: Pick<Room, 'openedBy' | 'participants'>,
): string | undefined {
  if (!room.openedBy) return undefined;
  return room.participants.some((participant) => participant.sessionId === room.openedBy)
    ? undefined
    : room.openedBy;
}
```

**ERROR_HANDLING:**

```typescript
// SOURCE: engine/src/kild/room/room-manager.ts:58-63
function fail<T>(
  code: 'not_found' | 'invalid_state' | 'rejected',
  message: string,
): CommandResult<T> {
  return { ok: false, code, message };
}
```

**LOGGING_PATTERN:**

```typescript
// SOURCE: engine/src/kild/sessions.ts:235-240
prompt(id: string, text: string, from?: string): boolean {
  const entry = this.sessions.get(id);
  if (!entry) return false;
  entry.session.prompt(text, from);
  return true;
}
```

No logger exists on this path. Mirror the explicit return-value/typed-result style rather than adding warnings or human fallback.

**REPOSITORY_PATTERN:**

```typescript
// SOURCE: engine/src/kild/sessions.ts:171-185
export class SessionManager {
  private readonly sessions = new Map<string, { session: PiSession; info: SessionInfo }>();
  private readonly subscribers = new Set<(msg: Outbound) => void>();

  list(): SessionInfo[] {
    return [...this.sessions.values()].map((s) => s.info);
  }
}
```

Session-owned lookup data already lives in `SessionManager`; extend that substrate instead of creating a parallel registry.

**SERVICE_PATTERN:**

```typescript
// SOURCE: engine/src/server.ts:303-314
app.post('/api/rooms/:id/post', async (c) => {
  const { text, from } = await c.req.json<{ text?: unknown; from?: unknown }>();
  if (typeof text !== 'string') return c.json({ error: 'text required' }, 400);
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  const id = c.req.param('id');
  const result =
    (from ?? HUMAN) === HUMAN
      ? await roomManager.postFromHuman(id, text)
      : await roomManager.postAs(id, from as string, text);
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
```

Preserve the route shape: parse `unknown`, validate locally, map typed room results through `roomResultStatus()`.

**TEST_STRUCTURE:**

```typescript
// SOURCE: engine/src/kild/sessions.test.ts:1-11
import { expect, test } from 'bun:test';

import { SessionManager } from './sessions.ts';

test('prompt silently drops a dead or missing session', () => {
  const sessions = new SessionManager();
  expect(sessions.prompt('missing', 'room closed', 'kild')).toBe(false);
});
```

And for room attribution-specific exact assertions:

```typescript
// SOURCE: engine/src/kild/room/room-manager.test.ts:307-325
expect(prompted).toEqual([
  {
    id: 'brain-session',
    from: 'kild',
    text: "[kild operator notification] Room 'demo': @worker posted to @human: @human approve the gate?",
  },
]);
```

---

## Files to Change

| File | Action | Justification |
| --- | --- | --- |
| `engine/src/kild/sessions.ts` | UPDATE | expose a live session→actor lookup from existing `SessionInfo` data |
| `engine/src/kild/sessions.test.ts` | UPDATE | cover lookup/rejection behavior in existing session-layer test style |
| `engine/src/kild/room/rest-room-attribution.ts` | CREATE | pure helper for the two in-scope REST attribution modes and typed rejections |
| `engine/src/kild/room/rest-room-attribution.test.ts` | CREATE | exact derivation/rejection tests without spinning up Hono or subprocesses |
| `engine/src/kild/fleet/engine-client.ts` | UPDATE | send `sessionId` on post/close requests; stop relying on `from` for success paths |
| `engine/src/kild/fleet/open-room-tool.ts` | UPDATE | stop sending kickoff `from`; rely on `openedBy` only |
| `engine/src/kild/fleet/post-room-tool.ts` | UPDATE | remove model-facing `from` schema and forward `KILD_SESSION_ID` |
| `engine/src/kild/fleet/close-room-tool.ts` | UPDATE | attach `KILD_SESSION_ID` to close requests |
| `engine/src/server.ts` | UPDATE | derive actor identity for open/post/close and reject mixed or unknown identity inputs |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- No capability-assembly / worker-tool-modes refactor. This slice supports only two successful modes: session-aware actor derivation and sessionless human fallback.
- No new room lifecycle/event transport, no room-manager behavior change beyond who the server passes into existing methods, and no cockpit/WebSocket protocol changes.
- No broader command-surface unification from slice 6. This plan stays on the current room REST endpoints and fleet tools only.
- No “named sessionless agent” mode. Actor identity must come from engine session state, not free-form REST/tool prose.

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### `[x]` Task 1: UPDATE `engine/src/kild/sessions.ts`

- **ACTION**: ADD a small live-session actor lookup on `SessionManager`
- **IMPLEMENT**: Expose a method or helper that returns the stored `SessionInfo` (or directly the actor name) for a given session id so server attribution code can derive `agent` from engine-owned state. Treat an unknown id, or a live session with no usable actor identity, as an explicit non-success result for callers to convert into typed room rejection.
- **MIRROR**: `engine/src/kild/sessions.ts:171-185` — session metadata already lives in one private map; extend that substrate rather than creating another registry.
- **IMPORTS**: reuse existing `SessionInfo` and `CommandResult` types if needed; do not introduce a new dependency.
- **GOTCHA**: derive actor from engine session metadata, not from `KILD_PARTICIPANT`, request env, or any REST body field. The audit target is engine-derived identity, not another caller-controlled string path.
- **VALIDATE**: `cd engine && bun test src/kild/sessions.test.ts`

### `[x]` Task 2: UPDATE `engine/src/kild/sessions.test.ts`

- **ACTION**: ADD session attribution regression tests in the existing tiny-unit style
- **IMPLEMENT**: Cover the new lookup helper for: known session with agent identity, unknown session id, and any “live session exists but has no actor identity” edge the implementation chooses to reject. Keep tests isolated to `SessionManager` behavior, not worker subprocesses.
- **MIRROR**: `engine/src/kild/sessions.test.ts:1-11` — short `bun:test` cases with exact boolean/value assertions.
- **PATTERN**: If the helper needs a seeded live session, follow the codebase’s normal dependency-injection/unit pattern rather than starting the full server.
- **GOTCHA**: preserve the existing `prompt()` missing-session no-op contract; this slice is about attribution resolution, not session delivery behavior.
- **VALIDATE**: `cd engine && bun test src/kild/sessions.test.ts`

### `[x]` Task 3: CREATE `engine/src/kild/room/rest-room-attribution.ts`

- **ACTION**: CREATE a pure REST attribution helper for room routes
- **IMPLEMENT**: Model the two success modes explicitly: (1) no session identity field present → `human`; (2) valid session identity present (`openedBy` for room open, `sessionId` for post/close) → derived agent name. Return typed `rejected` failures for mixed identity (`session identity` + legacy `from`), unknown session identity, and any live session that cannot produce an actor name. Keep the helper transport-neutral enough that `server.ts` only parses JSON and maps the helper’s result to `roomManager` calls.
- **MIRROR**: `engine/src/kild/room/room-events.ts:20-41` — small pure helper module with deterministic branches and exact return values; `engine/src/kild/room/room-manager.ts:58-63` — use typed `rejected` failures instead of booleans or exceptions for domain-invalid identity.
- **TYPES**: define separate request input types for `open`, `post`, and `close` so `openedBy` and `sessionId` stay distinct and the slice does not smuggle in a wider command abstraction.
- **GOTCHA**: do not quietly preserve a third “sessionless named agent” path. This slice is intentionally only `session-derived actor` or `human`.
- **VALIDATE**: `cd engine && bun run typecheck`

### `[x]` Task 4: CREATE `engine/src/kild/room/rest-room-attribution.test.ts`

- **ACTION**: ADD exact derivation/rejection tests for the pure helper
- **IMPLEMENT**: Assert: sessionless room-open/post/close resolve to `human`; room-open derives kickoff actor from `openedBy` session -> agent name; post/close derive actor from `sessionId`; requests with both session identity and `from` reject; unknown session id rejects; a session-aware request never falls back to `human`. If the helper rejects legacy `from` even without session identity, lock that behavior with explicit tests and explain it in Agent Notes.
- **MIRROR**: `engine/src/kild/room/room-events.test.ts:28-64` and `engine/src/kild/models.test.ts:13-31` — exact-output/tests for positive and negative cases, including “no silent fallback” assertions.
- **PATTERN**: Stub the session lookup as a tiny pure callback/object map; do not stand up Hono or worker processes.
- **GOTCHA**: include a regression for the staged audit rule that unknown session identity is a typed rejection, never human fallback.
- **VALIDATE**: `cd engine && bun test src/kild/room/rest-room-attribution.test.ts`

### `[x]` Task 5: UPDATE fleet tool/client files

- **ACTION**: UPDATE `engine/src/kild/fleet/engine-client.ts`, `open-room-tool.ts`, `post-room-tool.ts`, and `close-room-tool.ts`
- **IMPLEMENT**: Remove `from` from model-facing TypeBox schemas and tool parameter handling. `open_room` should send only `openedBy: process.env.KILD_SESSION_ID` for session-aware attribution. `post_room` should send `{ text, sessionId }`, and `close_room` should send `{ sessionId }`, both sourced from the manager-owned env when present. Update request interfaces and descriptions so they describe engine-derived identity, not sender overrides.
- **MIRROR**: `engine/src/kild/fleet/open-room-tool.ts:30-39` — read `process.env.KILD_SESSION_ID` inside the tool, not from model input; `engine/src/kild/fleet/engine-client.ts:38-63` — request bodies are simple typed JSON wrappers over existing endpoints.
- **IMPORTS**: keep using `Type` from `typebox` and existing `openRoom` / `postRoom` / `closeRoom` client helpers.
- **GOTCHA**: `open_room` already has `openedBy` from slice 8a; do not rename it to `sessionId` and accidentally blur opener ownership with action attribution.
- **VALIDATE**: `cd engine && bun run typecheck && bun run lint`

### `[x]` Task 6: UPDATE `engine/src/server.ts`

- **ACTION**: DERIVE room actor attribution in the REST boundary
- **IMPLEMENT**: Keep current `unknown` JSON parsing style, but route `/api/rooms`, `/api/rooms/:id/post`, and `/api/rooms/:id/close` through the new attribution helper. For room open, resolve kickoff speaker from `openedBy` session -> agent name and call `roomManager.postAs()` only when session-aware attribution succeeds; otherwise use `postFromHuman()` for the sessionless case. For post and close, accept optional `sessionId`, derive actor for post, and reject bad/mixed identity inputs with typed `rejected` results mapped by `roomResultStatus()`. Session-aware requests must never fall back to `human` when lookup fails.
- **MIRROR**: `engine/src/server.ts:246-319` — local request validation plus `roomResultStatus()` mapping; `engine/src/kild/room/room-manager.ts:163-176, 335-356` — keep using existing room-manager entry points rather than inventing parallel room commands.
- **PATTERN**: If the route needs a small adapter around the pure helper result, keep it as a local server function near `participantSpecs()` / `envRecord()` / `addressKickoff()`.
- **GOTCHA**: preserve sessionless REST as `human` for cockpit/CLI compatibility, but reject any session-aware request that cannot be resolved. This is the audit’s “two modes only” boundary.
- **VALIDATE**: `cd engine && bun test src/kild/sessions.test.ts src/kild/room/rest-room-attribution.test.ts src/kild/room/room-manager.test.ts && bun run typecheck && bun run lint`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
| --- | --- | --- |
| `engine/src/kild/sessions.test.ts` | known actor lookup; unknown session; no-actor session rejection | session-owned attribution substrate |
| `engine/src/kild/room/rest-room-attribution.test.ts` | sessionless human fallback; open/post/close actor derivation; mixed identity rejection | pure REST attribution rules |
| `engine/src/kild/room/rest-room-attribution.test.ts` | unknown session id and no-human-fallback assertions | typed rejection semantics required by the audit |
| `engine/src/kild/room/room-manager.test.ts` | existing attribution tests remain green | server changes did not require room-manager semantic drift |

### Edge Cases Checklist

- [ ] `KILD_SESSION_ID` remains manager-owned and is read from env, not tool params.
- [ ] `open_room` kickoff attribution derives from `openedBy` session -> agent name, not `from`.
- [ ] `post_room` and `close_room` send session identity on the REST body.
- [ ] Sessionless REST requests with no identity field still post/close as `human` for cockpit/CLI compatibility.
- [ ] Any request carrying both session identity and `from` is rejected with a typed `rejected` result.
- [ ] Unknown session identity is rejected and never falls back to `human`.
- [ ] A live session with missing actor metadata is handled explicitly (rejected), not guessed.
- [ ] No capability-assembly / worker-tool-modes generalization leaks into this slice.

---

## Validation Commands

🔁 **Validation loop:** kild worktrees do not share `node_modules`; run setup first in `engine`, then rerun failing checks until exit 0.

### Level 1: STATIC_ANALYSIS

```bash
cd engine && bun install && bun run typecheck && bun run lint
```

**EXPECT**: Exit 0; TypeScript and Biome pass after the attribution changes.

### Level 2: UNIT_TESTS

```bash
cd engine && bun test src/kild/sessions.test.ts src/kild/room/rest-room-attribution.test.ts src/kild/room/room-manager.test.ts
```

**EXPECT**: New derivation/rejection tests and existing room-manager regressions pass.

### Level 3: FULL_SUITE

```bash
cd engine && bun test && bun run typecheck && bun run lint
```

**EXPECT**: Full engine suite stays green; no build step exists in `engine/package.json`.

### Level 4: DATABASE_VALIDATION

Not applicable — no database/schema work.

### Level 5: BROWSER_VALIDATION

Not applicable — cockpit and room WebSocket protocol stay unchanged.

### Level 6: MANUAL_VALIDATION

1. Run `cd engine && bun install` if this worktree lacks dependencies.
2. Start the engine and a live fleet brain session.
3. Open a room through `open_room`; verify the kickoff is attributed from the opener session’s agent name, not client-provided `from`.
4. Post to the room with `post_room`; verify the server derives the actor from the live session id.
5. Close the room with `close_room`; verify the close succeeds only with a known live session id.
6. Retry post/open/close with an unknown session id and with both `sessionId|openedBy` plus `from`; verify typed rejection and no human fallback.

---

## Acceptance Criteria

- [ ] Fleet tools send manager-owned `KILD_SESSION_ID` identity with post and close requests.
- [ ] Server resolves session id -> agent name for room open kickoff attribution and room post attribution.
- [ ] `from` is removed from model-facing fleet tool schemas.
- [ ] Sessionless REST remains `human` for cockpit/CLI compatibility.
- [ ] Requests carrying both session identity and `from` are rejected.
- [ ] Unknown session identity is a typed rejection, never a human fallback.
- [ ] Room-open kickoff attribution derives from `openedBy` session -> agent name, replacing `from`.
- [ ] Derivation/rejection tests exist in the codebase’s current pure/unit style.
- [ ] Later implementation done criteria remain: `cd engine && bun run typecheck && bun run lint && bun test` green; committed; no push/PR/merge.

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] `cd engine && bun install` run before validation if this worktree lacks `node_modules`
- [ ] Level 1: Static analysis passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite passes
- [ ] Levels 4-5 correctly omitted as not applicable
- [ ] Implementation committed locally with no push/PR/merge

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| Server derives actor from the wrong session field or wrong metadata property | MED | HIGH | Keep a pure attribution helper with explicit open/post/close input types and exact unit tests for each path |
| Legacy `from` handling survives on one route and leaves a third attribution mode | MED | HIGH | Centralize route attribution logic and assert mixed/legacy identity behavior in one dedicated test file |
| Unknown session id silently becomes `human` | MED | HIGH | Add explicit “no silent fallback” tests modeled after `models.test.ts` and map failures through typed `rejected` results |
| Slice grows into a generalized command/capability refactor | HIGH | MED | Keep new code scoped to room REST attribution only; defer command-surface unification to slice 6 |
| Session metadata lacks an agent name for some live sessions | LOW | MED | Reject explicitly and document that only engine-known actor sessions participate in the session-aware mode |

---

## Questionables

<details>
<summary>Should legacy sessionless `from` without a session id be rejected or ignored?</summary>

Recommended assumption: reject it and keep only the two in-scope success modes (`human` with no identity field, or session-derived actor with a known session id). This best matches the audit statement that actor identity must come from the engine, not client-provided prose. If implementation chooses this path, lock it with a dedicated helper test and document it in the final report.

</details>

---

## Agent Notes

**Approach chosen:** add a pure attribution decision module and a small `SessionManager` lookup seam, then keep `server.ts` as the only transport boundary. This fits the current codebase style better than introducing Hono integration tests or a larger command abstraction.

**Why this fits existing architecture:**

- `KILD_SESSION_ID` is already manager-owned in `engine/src/kild/sessions.ts:84-97`; slice 8a proved the env path.
- Server routes already validate JSON locally and map typed results with `roomResultStatus()` in `engine/src/server.ts:246-319`.
- Room behavior already separates `postFromHuman()` from `postAs()` in `engine/src/kild/room/room-manager.ts:163-176`, so only the attribution choice needs to move.
- The codebase prefers small exact unit tests (`sessions.test.ts`, `models.test.ts`, `room-events.test.ts`) over full app/request integration tests for slices like this.

**Alternatives rejected:**

- **Broader capability-assembly / worker-tool-modes refactor now:** explicitly out of scope; this is slice 6 material.
- **Server route tests that instantiate Hono app/request:** possible, but it would add new test style where current code already supports a purer helper seam.
- **Deriving actor from request `env` / room participant handles / free-form `from`:** rejected because those are not engine-owned identity.
- **New room-manager APIs for session-aware actions:** unnecessary; the server can derive speaker identity and keep calling existing room-manager methods.

**Discovery table:**

| Category | File:Lines | Pattern Description | Evidence |
| --- | --- | --- | --- |
| Session identity | `engine/src/kild/sessions.ts:84-97` | manager overrides worker env with `KILD_SESSION_ID` | `KILD_SESSION_ID: id` after spreading `req.env` |
| Session metadata | `engine/src/kild/sessions.ts:187-232` | live session info is stored centrally by id | `this.sessions.set(id, { session, info })` |
| Open-room identity | `engine/src/kild/fleet/open-room-tool.ts:30-39` | room-open already ships `openedBy` from env | `...(sessionId ? { openedBy: sessionId } : {})` |
| Legacy free-form actor | `engine/src/kild/fleet/post-room-tool.ts:10-23` | post tool exposes optional `from` override | `from: Type.Optional(Type.String(...))` |
| REST attribution today | `engine/src/server.ts:292-296, 303-313` | server trusts `from` and otherwise falls back to `human` | `typeof body.from === 'string' ... postAs ... : postFromHuman` |
| Room command seam | `engine/src/kild/room/room-manager.ts:163-176, 347-356` | server only needs to choose human vs actor before invoking room manager | existing `postFromHuman` / `postAs` / `close` entry points |
| Existing-style tests | `engine/src/kild/sessions.test.ts:1-11`, `engine/src/kild/models.test.ts:13-31`, `engine/src/kild/room/room-events.test.ts:28-64` | small exact-output unit tests for no-fallback semantics | missing-session + unknown-model + opener-target cases |

**Confidence:** 9/10 for one-pass implementation success. The only load-bearing decision is whether to reject legacy sessionless `from` entirely or only the mixed case; the recommended two-mode-only answer is documented above.

---

## Amendments

<details>
<summary>2026-07-17T13:55:52Z — Initial plan</summary>

Plan-only artifact for primitives-audit slice 5. No implementation changes included.

</details>
