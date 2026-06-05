# Feature: Worktree-per-session isolation

## Summary

Let each agent session optionally run in its own isolated `kild/<name>` **git
worktree** instead of the raw project directory, so concurrent agents in the same
repo don't trample each other's files. Surface the branch + worktree path in the
cockpit (and CLI), with cleanup on explicit close. The work is split along a
deliberate boundary: **kild owns the policy** (branch/path conventions, the
session-path wiring, cleanup, the cockpit UI) and **a self-contained `worktree()`
SandboxFactory is built as a Flue-promotable mechanism** that can later be lifted
upstream into Flue's sandbox abstraction.

## User Story

As a power user orchestrating many agents,
I want each agent to work in its own isolated git branch/worktree,
So that I can run several agents on one repo in parallel without them corrupting
each other's working tree — and review/merge each agent's branch afterward.

## Problem Statement

Concurrency is real (subprocess-per-session, shipped), but every session's `cwd`
is the **raw project dir** (`engine/src/kild/sessions.ts:40` → `KILD_CWD` →
`engine/src/worker.ts:15` → `createAgentSession({cwd})`). Two agents in the same
project write over each other — so today's concurrency is a footgun. There is no
isolation, no branch-per-agent, and no way to view/merge an agent's output as a
branch.

## Solution Statement

A session spawn carries an optional `worktree` **name**. Absent → the agent runs in
the project's **main/dev checkout** (cwd = project dir; the default — planners,
reviews-of-main, the brain). Present → the agent runs in the `kild/<name>` worktree;
the **worker** *ensures* it (creates it if missing, **attaches** to it if it already
exists — so a reviewer named the same worktree as the coder works in the same tree).
Worker-side creation keeps `SessionManager.spawn` synchronous (no re-introduced race;
async git in the worker; stdin buffers early prompts). The engine derives the
deterministic worktree path for `SessionInfo` so the cockpit updates instantly.
Worktrees **persist** (removal is an explicit action, never on session close — a
shared worktree must survive any one session closing); the git domain logic lives in
kild (no `@flue` on the hot path); a self-contained `worktree()` factory is the Flue
contribution.

## Policy model — worktree is selectable, shareable, persistent

A worktree is a **named resource** (`<name>` → branch `kild/<name>` → dir
`$KILD_HOME/worktrees/<name>`), not 1:1 with a session. A session *selects* where it
runs:

| Selection (`worktree` field) | cwd | Use |
|---|---|---|
| *absent* | project main/dev checkout | planning agent, brain, review-of-main |
| `"fix-auth"` (new) | fresh `kild/fix-auth` worktree | coding agent |
| `"fix-auth"` (exists) | the **same** `kild/fix-auth` worktree (attach) | reviewer joining the coder's tree |

- **Same name = shared; different names = split.** This single field expresses all of
  it — and is exactly what a future **room** needs: one shared name for the room, or a
  name per agent, with no new primitive. Agents can also self-create worktrees (the
  brain's `create_worktree` tool) and spawn sub-sessions into them.
- **`ensureWorktree` (create-or-attach), not force-create.** The session path must NOT
  reset an existing worktree (that would blow away a coder's work when a reviewer
  joins). `createWorktree` (force `-B`) stays for explicit "new worktree"; the session
  path uses `ensureWorktree`.
- **Worktrees persist; removal is explicit** — `kild worktree rm` / a UI action. **The
  one automatic path in v1:** a worktree whose `kild/<name>` branch is **merged** into
  the repo's default branch is auto-pruned (worktree removed + branch `-d` deleted) —
  checked on engine start and on each worktree-list refresh (via `git branch --merged
  <default>`). No remove-on-close, no refcounting, **no background polling**.

## Metadata

| Field            | Value                                                            |
| ---------------- | --------------------------------------------------------------- |
| Type             | NEW_CAPABILITY                                                  |
| Complexity       | MEDIUM                                                          |
| Systems Affected | engine (worktree, sessions, worker, server, cli), cockpit (types, api, +page, Sidebar/Topbar), flue layer (sandbox), Tauri capabilities (optional open) |
| Dependencies     | bun, `@earendil-works/pi-coding-agent` (cwd), `@flue/runtime` (Flue layer only), git CLI |
| Estimated Tasks  | ~22 across 6 phases                                            |

---

## The boundary (the whole point)

| | **kild-owned (POLICY + product)** | **Flue-promotable (general MECHANISM)** |
|---|---|---|
| What | `kild/<name>` naming, `$KILD_HOME/worktrees/<name>` path, validation, git CRUD + `ensureWorktree` (create-or-attach), SDK-session-path wiring (worker ensures from `KILD_WORKTREE`), `SessionInfo.branch/worktreePath`, **persist + merge-prune + explicit `rm`** policy, cockpit "Run in" selector + chip + Worktrees panel, CLI `--worktree` + `kild worktree ls/rm/prune` | A standalone `worktree({ repo, branch, root })` → `{ sandbox, cleanup() }` — creates on `createSessionEnv`, caller-managed `cleanup()` (Flue has no teardown hook — see mapping), parameterized, **no kild conventions**, liftable into Flue verbatim |
| Where | `engine/src/kild/worktree.ts` (de-Flue'd), `engine/src/worker.ts`, `engine/src/kild/sessions.ts`, `engine/src/server.ts`, `engine/src/cli.ts`, `app/src/**` | `engine/src/flue/worktree-sandbox.ts` (new), used by the Flue layer (`brain`/`workflows`) + packaged as the upstream reference |
| Hot path? | YES — the shipped session path (coding-agent SDK + `cwd`) | NO — Flue sandboxes aren't on kild's session hot path |
| Depends on `@flue/runtime`? | **No** (currently `worktree.ts` wrongly does; this plan removes it) | Yes (it *is* a Flue sandbox) |

---

## Flue codebase mapping (verified against `github.com/withastro/flue` source)

The Flue-promotable module must map onto Flue's **real** interface. Verified facts:

- **`SandboxFactory`** (`packages/runtime/src/types.ts:923`):
  ```ts
  export interface SandboxFactory {
    createSessionEnv(options: { id: string }): Promise<SessionEnv>;
    tools?: SessionToolFactory; // replaces the default tool list for this sandbox
  }
  ```
  Note: `createSessionEnv` receives `{ id }` (a session id), **not** `{ cwd }`.
- **`SessionEnv`** (`packages/runtime/src/types.ts:265`): `{ exec, readFile, readFileBuffer, writeFile, stat, readdir, exists, mkdir, rm, cwd, resolvePath }`. **There is NO `dispose`/`close`/`teardown` method.**
- **`local()`** (`packages/runtime/src/node/local.ts`) is the exact pattern to mirror:
  ```ts
  export function local(options = {}): SandboxFactory {
    return { createSessionEnv: async () => createLocalSessionEnv(options) };
  }
  ```
- **Flue does not tear sandboxes down.** No call site disposes a `SessionEnv` at session end (the Daytona example has the *caller* create AND destroy the container). **Sandbox resource lifecycle is caller-managed.**

**Consequence for the design:**
- A Flue `worktree()` factory can **create** the worktree in `createSessionEnv` and return `local({ cwd })` — but it **cannot auto-remove** the worktree on session end, because Flue exposes no end-of-life hook. So the contribution is `worktree({repo, branch, root?})` returning **`{ sandbox: SandboxFactory; cleanup(): Promise<void> }`** (caller-managed), matching Flue's existing model.
- **This is itself the more valuable contribution:** the Flue issue proposes BOTH the `worktree()` sandbox **and** an optional teardown hook (`SessionEnv.dispose?()` or a `SandboxFactory.teardown?(env)`), since *every* resource-owning sandbox (worktree, ephemeral container, microVM) hits this gap. We surface the gap with a concrete use-case + reference impl.
- **kild is unaffected on its hot path:** kild does NOT use Flue sandboxes for sessions (it uses the coding-agent SDK + `cwd` and manages worktree lifecycle itself — worker creates, `SessionManager.stop` removes). So kild's cleanup works regardless of Flue's missing hook. The boundary holds.

---

## UX Design

### Before State
```
╔════════════════════════════════════════════════════════════════════════╗
║  BEFORE — every session runs in the raw project dir (no isolation)       ║
╠════════════════════════════════════════════════════════════════════════╣
║  [+ start session]  ──►  spawn{cwd: project.path}  ──►  worker cwd =     ║
║                                                        project.path      ║
║                                                                          ║
║   Agent A ─┐                                                             ║
║            ├──►  /Users/me/proj  (SAME working tree)  ◄── COLLISION      ║
║   Agent B ─┘                                                             ║
║                                                                          ║
║  PAIN: two agents on one repo overwrite each other; no branch to review. ║
╚════════════════════════════════════════════════════════════════════════╝
```

### After State
```
╔════════════════════════════════════════════════════════════════════════╗
║  AFTER — each session SELECTS where it runs (main / new / existing wt)   ║
╠════════════════════════════════════════════════════════════════════════╣
║  [+ start session]   Run in: ( main checkout ▾ | worktree: [fix-auth] )  ║
║                                                                          ║
║   planner   ─ worktree:absent ─►  /Users/me/proj        (main/dev)       ║
║   brain     ─ worktree:absent ─►  /Users/me/proj        (main/dev)       ║
║   coder     ─ worktree:fix-auth ─► …/worktrees/fix-auth (kild/fix-auth)  ║
║   reviewer  ─ worktree:fix-auth ─► …/worktrees/fix-auth (SAME — attach)  ║
║   coder-2   ─ worktree:refactor ─► …/worktrees/refactor (split)          ║
║                                                                          ║
║  worker: ensureWorktree(repo,name) — create if missing, ATTACH if exists ║
║  Topbar:  proj · agent · model   [⎇ kild/fix-auth]  [open ⧉]            ║
║  worktrees PERSIST — removed only via `kild worktree rm` / UI action     ║
║                                                                          ║
║  VALUE: planners/brain on main; coders isolated; reviewers share the     ║
║  coder's tree; nothing auto-deleted out from under a shared worktree.    ║
╚════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes
| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| Sidebar new-session form | agent + model | + "isolate in worktree" toggle + branch name | Opt into per-agent isolation |
| `Topbar.svelte` | proj · agent · model · stats | + `⎇ kild/<branch>` chip + "open worktree" | See/open the agent's branch |
| `kild run` (CLI) | `--project --agent --model` | + `--branch <name>` | Headless isolated runs |
| On session close (✕) | nothing extra | engine removes the worktree; branch persists | Disk freed, work preserved as a branch |

---

## Mandatory Reading

| Priority | File | Lines | Why |
|----------|------|-------|-----|
| P0 | `engine/src/kild/worktree.ts` | 1-65 | The module to SPLIT (remove `@flue` from domain logic; add path helpers) |
| P0 | `engine/src/kild/sessions.ts` | 5-23, 34-44, 99-129 | `SpawnRequest`/`SessionInfo`/`Outbound`; `PiSession` env; `SessionManager.spawn/stop` — where `branch` threads in |
| P0 | `engine/src/worker.ts` | 14-34 | Where `KILD_CWD` is read → `createAgentSession({cwd})`; add the worktree-create step |
| P1 | `engine/src/server.ts` | 60-71, 100-109 | `ClientMessage` + `parseClientMessage` (validate the new `branch`) |
| P1 | `engine/src/cli.ts` | 16-24, 104-126, 164-176 | `parseArgs` options + `runViaEngine`/`runInProcess` cwd resolution |
| P1 | `engine/src/kild/config.ts` | 1-13 | `kildHome()` (worktree root base) |
| P2 | `engine/src/kild/events.test.ts` | 1-41 | `bun:test` pattern to mirror |
| P2 | `app/src/lib/types.ts` | 20-42 | `Session` + cockpit `SessionInfo` (add branch fields) |
| P2 | `app/src/lib/api.ts` | 32, 90-92 | `SpawnOptions` + `EngineSocket.spawn` |
| P2 | `app/src/routes/+page.svelte` | 131-151, 184-206 | `startSession` + `reconcileSessions` |
| P2 | `app/src/lib/components/Topbar.svelte` | 8-19 | Where a branch chip + open action go |
| P3 | `app/src-tauri/capabilities/default.json` | 6-17 | Current `opener` perms (no `allow-open-path` yet) — for the optional "open worktree" |

**External docs:** none required — git worktree semantics + existing deps. Gotchas captured inline below.

---

## Patterns to Mirror

**GIT-VIA-execFile (kild-owned; already correct — keep this style):**
```typescript
// SOURCE: engine/src/kild/worktree.ts:24-39
function assertSafeBranch(branch: string): void {
  if (branch.startsWith('-') || !/^[A-Za-z0-9._/-]+$/.test(branch)) {
    throw new Error(`invalid branch name: ${branch}`);
  }
}
export async function createWorktree(repo: string, branch: string): Promise<Worktree> {
  assertSafeBranch(branch);
  const wtPath = path.join(worktreesRoot(), branch.replace(/\//g, '-'));
  const ref = `kild/${branch}`;
  await execFile('git', ['-C', repo, 'worktree', 'remove', '--force', wtPath]).catch(() => {});
  await execFile('git', ['-C', repo, 'worktree', 'add', '-B', ref, wtPath]);
  return { branch: ref, path: wtPath };
}
```

**WORKER ENV → cwd (where the worktree-create step inserts):**
```typescript
// SOURCE: engine/src/worker.ts:15, 29
const cwd = process.env.KILD_CWD || process.cwd();
// ...
({ session } = await createAgentSession({ model, authStorage, modelRegistry: registry, cwd }));
```

**PiSession ENV (where KILD_BRANCH is added):**
```typescript
// SOURCE: engine/src/kild/sessions.ts:34-44
this.child = spawn(process.argv[0] as string, process.argv.slice(1), {
  env: { ...process.env, KILD_ROLE: 'worker', KILD_MODEL: req.model ?? '',
         KILD_CWD: req.cwd ?? process.cwd(), KILD_AGENT: req.agent ?? '' },
  stdio: ['pipe', 'pipe', 'inherit'],
});
```

**SessionInfo construction (where branch/worktreePath are added):**
```typescript
// SOURCE: engine/src/kild/sessions.ts:101-108
const info: SessionInfo = {
  id, model: req.model, cwd: req.cwd, agent: req.agent, projectName: req.projectName, origin,
};
```

**WS validation (mirror for the new `branch` field):**
```typescript
// SOURCE: engine/src/server.ts (parseClientMessage)
if (typeof m.id !== 'string') return null;
if (m.type === 'spawn' || m.type === 'stop') return m as ClientMessage;
```

**TEST STRUCTURE (bun:test, flat tests):**
```typescript
// SOURCE: engine/src/kild/events.test.ts:1-12
import { expect, test } from 'bun:test';
import { translate } from './events.ts';
test('text_delta becomes a text event', () => {
  expect(translate({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'hi' } }))
    .toEqual({ kind: 'text', delta: 'hi' });
});
```

**FLUE SANDBOX SEED (the mechanism to graduate into `flue/worktree-sandbox.ts`):**
```typescript
// SOURCE: engine/src/kild/worktree.ts:54-56  (move to flue/, make lifecycle-managed)
export function worktreeSandbox(wt: Worktree): SandboxFactory {
  return local({ cwd: wt.path });
}
```

**SANDBOXFACTORY SHAPE (from the spike research — Flue's interface):**
```typescript
// Flue SandboxFactory: { createSessionEnv: async () => SessionEnv }
// `local({cwd})` already returns one; a worktree() factory wraps create+local+teardown.
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `engine/src/kild/worktree.ts` | UPDATE | Remove `@flue` imports + `worktreeSandbox` (move to flue/); add `worktreePath(name)` + `worktreeRef(name)` deterministic helpers; keep CRUD. Now kild-owned, hot-path-safe |
| `engine/src/kild/worktree.test.ts` | CREATE | Deterministic tests: branch validation, path/ref derivation |
| `engine/src/worker.ts` | UPDATE | If `KILD_WORKTREE` set, `ensureWorktree(KILD_CWD, name)` (create-or-attach) → use its path as `cwd`; emit `error` on failure |
| `engine/src/kild/sessions.ts` | UPDATE | `SpawnRequest.worktree?`; `SessionInfo.branch?/worktreePath?`; set `KILD_WORKTREE` env; fill SessionInfo via `worktreeRef/worktreePath`; **does NOT remove worktrees on stop** |
| `engine/src/server.ts` | UPDATE | `ClientMessage.spawn.worktree?` (validate); `GET/DELETE /api/worktrees` + prune-merged on start/list; (optional) `POST /api/open` |
| `engine/src/cli.ts` | UPDATE | `run --worktree`; a `kild worktree ls/rm/prune` command group |
| `app/` worktrees UI | UPDATE | "Run in" selector (main / new / existing worktree) + a Worktrees panel (list + ✕ remove + prune) |
| `engine/src/server.ts` | UPDATE | (optional, Phase 4) `POST /api/open` — reveal a worktree path in the OS, scoped to the worktree root |
| `engine/src/cli.ts` | UPDATE | `--branch` option; pass through in `runViaEngine`/`runInProcess` |
| `engine/src/flue/worktree-sandbox.ts` | CREATE | **Flue-promotable** `worktree()` → `{sandbox, cleanup}` (caller-managed; the contribution) |
| `engine/src/flue/worktree-sandbox.test.ts` | CREATE | Deterministic test (no LLM): factory creates a worktree, cwd is inside it, teardown removes it |
| `engine/src/kild/brain.ts` | UPDATE | `create_worktree` tool uses kild `worktree.ts` (no behavior change) |
| `engine/src/workflows/worktree-demo.ts` | UPDATE | Import `worktreeSandbox` from `flue/worktree-sandbox.ts` |
| `app/src/lib/types.ts` | UPDATE | `SessionInfo.branch?/worktreePath?`; `Session.branch?/worktreePath?` |
| `app/src/lib/api.ts` | UPDATE | `SpawnOptions.branch?` |
| `app/src/routes/+page.svelte` | UPDATE | New-session branch toggle/state; pass `branch` in `startSession`; carry branch/worktreePath through `reconcileSessions` + the local `Session` push |
| `app/src/lib/components/Sidebar.svelte` | UPDATE | New-session form: "isolate in worktree" toggle + branch input |
| `app/src/lib/components/Topbar.svelte` | UPDATE | `⎇ kild/<branch>` chip + "open worktree" action |
| `.claude/skills/kild-cli/SKILL.md` | UPDATE | Document `kild run --branch` |
| `CLAUDE.md` | UPDATE | Note the worktree slice + the kild/Flue worktree boundary; protocol gains `branch` |

---

## NOT Building (Scope Limits)

- **Multi-repo / `Project.roots`** — deferred. The worktree primitive is per-repo; a project spanning N repos maps over it later (`Project.roots`, N worktrees, a workspace cwd). YAGNI until a real multi-repo project drives it.
- **Rooms** — the room-level "one shared worktree vs split per agent" selection is future. It needs **zero new primitives**: a room picks one `worktree` name (shared) or a name per agent (split); agents self-create via the brain's `create_worktree`. Not built now.
- **Default-on isolation** — the worktree is **opt-in** ("Run in" defaults to main checkout / `--worktree` absent). Auto-isolating coders by default is a fast-follow.
- **Remove-on-close / refcounting** — sessions never remove worktrees; worktrees persist. The **only** automatic removal is the merge-prune (Task 2.8). No refcounting, no background polling.
- **Auto-commit / dirty-tree safety** — explicit `rm` does `worktree remove --force` (discards uncommitted work). Merge-prune is safe (merged ⇒ integrated). A "commit/stash before rm" net is future work.
- **The actual Flue PR being merged** — Phase 6 produces the reference impl + opens the issue; acceptance into Flue is out of our control.

---

## Step-by-Step Tasks

### PHASE 1 — kild-owned: make `worktree.ts` hot-path-safe (de-Flue the domain logic)

**Task 1.1: UPDATE `engine/src/kild/worktree.ts`**
- **ACTION**: Remove `import type { SandboxFactory } from '@flue/runtime'` and `import { local } from '@flue/runtime/node'`, and **delete** `worktreeSandbox()` (it moves to `flue/` in Phase 5). Add deterministic path helpers + a create-or-attach `ensureWorktree`:
- **IMPLEMENT**:
  ```typescript
  export function worktreeRef(name: string): string { assertSafeBranch(name); return `kild/${name}`; }
  export function worktreePath(name: string): string {
    assertSafeBranch(name);
    return path.join(worktreesRoot(), name.replace(/\//g, '-'));
  }
  /** Create the worktree if missing, ATTACH (reuse) if it already exists. The
   *  session path uses this — it must never reset a shared worktree. */
  export async function ensureWorktree(repo: string, name: string): Promise<Worktree> {
    const wtPath = worktreePath(name);
    const ref = worktreeRef(name);
    if (existsSync(wtPath)) return { branch: ref, path: wtPath }; // attach
    await execFile('git', ['-C', repo, 'worktree', 'add', '-B', ref, wtPath]); // create (no pre-remove)
    return { branch: ref, path: wtPath };
  }
  ```
  Keep `createWorktree` (force `-B` + pre-remove) for the brain's explicit "new worktree". `export assertSafeBranch` for the server.
- **MIRROR**: existing `engine/src/kild/worktree.ts:24-39`
- **GOTCHA**: `ensureWorktree` must NOT pre-remove (attach case). `createWorktree` keeps its force-remove (fresh). Import `existsSync` from `node:fs`.
- **VALIDATE**: `cd engine && bun run typecheck && bun run lint`

**Task 1.2: CREATE `engine/src/kild/worktree.test.ts`**
- **ACTION**: Deterministic unit tests (no git/LLM): `assertSafeBranch` accepts `fix-auth`, `a/b`, rejects `--evil`, `$(x)`, `a b`; `worktreeRef('x') === 'kild/x'`; `worktreePath('a/b')` ends with `worktrees/a-b`.
- **MIRROR**: `engine/src/kild/events.test.ts:1-12`
- **VALIDATE**: `cd engine && bun test src/kild/worktree.test.ts`

### PHASE 2 — kild-owned: wire worktree into the session path

**Task 2.1: UPDATE `engine/src/kild/sessions.ts` (types + env)**
- **ACTION**: Add `worktree?: string` (the worktree *name*) to `SpawnRequest`; add `branch?: string; worktreePath?: string` to `SessionInfo`. In `PiSession` env, add `KILD_WORKTREE: req.worktree ?? ''`. Keep `KILD_CWD = req.cwd` as the **repo** path.
- **MIRROR**: `engine/src/kild/sessions.ts:5-20, 34-44`
- **GOTCHA**: the worktree is ensured *from* the repo in the worker — don't pre-resolve cwd here (keeps spawn sync → no race).
- **VALIDATE**: `bun run typecheck`

**Task 2.2: UPDATE `engine/src/kild/sessions.ts` (`SessionManager.spawn` fills branch/worktreePath)**
- **ACTION**: When `req.worktree`, set `info.branch = worktreeRef(req.worktree)` and `info.worktreePath = worktreePath(req.worktree)` (deterministic, no await). Import from `./worktree.ts`.
- **MIRROR**: `engine/src/kild/sessions.ts:101-108`
- **GOTCHA**: those throw on a bad name → wrap in try/catch and broadcast an `error` event for that id rather than throwing out of `spawn`.
- **VALIDATE**: `bun run typecheck`

**Task 2.3: (no remove-on-close)** — `SessionManager.stop` does **NOT** touch the worktree. Worktrees persist; they're removed only via the explicit mechanism (Task 2.7). A shared worktree must survive any one session closing. *(This task is a deliberate non-change + a comment in `stop()`.)*

**Task 2.4: UPDATE `engine/src/worker.ts` (ensure the worktree)**
- **ACTION**: After reading `cwd`, read `const wt = process.env.KILD_WORKTREE || undefined;`. If set, `try { cwd = (await ensureWorktree(cwd, wt)).path } catch (err) { emit({kind:'error',message:\`worktree: ${errText(err)}\`}); process.exit(1) }` — BEFORE `createAgentSession`. (`ensureWorktree` = create-or-attach.)
- **MIRROR**: `engine/src/worker.ts:14-34`
- **IMPORTS**: `import { ensureWorktree } from './kild/worktree.ts'`
- **GOTCHA**: prompts sent before the worktree is ready are OS-buffered on stdin — no loss.
- **VALIDATE**: `bun run typecheck && bun run lint`

**Task 2.5: UPDATE `engine/src/server.ts` (accept + validate `worktree`)**
- **ACTION**: Add `worktree?: string` to `ClientMessage`'s `spawn` variant. In `parseClientMessage`, reject `m.worktree !== undefined && typeof m.worktree !== 'string'`.
- **MIRROR**: `engine/src/server.ts:60-71` + the existing `parseClientMessage` shape-checks
- **VALIDATE**: `bun run typecheck && bun run lint`

**Task 2.6: CREATE/extend `engine/src/kild/sessions.test.ts`**
- **ACTION**: Unit-test the pure derivations (worktree-name → ref → worktreePath in `SessionInfo`), no subprocess.
- **VALIDATE**: `bun test`

**Task 2.7: CREATE the separate worktree-management surface (engine)**
- **ACTION**: `GET /api/worktrees?project=<name>` → `listWorktrees(projectPath)` (existing) filtered to `kild/*`. `DELETE /api/worktrees` body `{ project, name }` → `removeWorktree(projectPath, worktreePath(name))` (frees disk; the branch persists). Add `removeWorktree`/`listWorktrees` are already in `worktree.ts`.
- **MIRROR**: existing route style `engine/src/server.ts:37-54`; reuse the `assertSafeBranch`/`worktreePath` helpers.
- **GOTCHA**: validate `name` (allowlist) before building the path; only operate under `worktreesRoot()`. Removing a worktree that a live session is using should warn/refuse if any session's `info.worktree === name` (cross-check `sessionManager.list()`).
- **VALIDATE**: `bun run typecheck && bun run lint`

**Task 2.8: CREATE the one automatic cleanup — prune merged worktrees (kild-owned)**
- **ACTION**: In `engine/src/kild/worktree.ts`, add `pruneMergedWorktrees(repo: string): Promise<string[]>` — detect the repo's default branch (`git rev-parse --abbrev-ref origin/HEAD` → fallback `main`/`master`/current), list branches merged into it (`git branch --merged <default>`), and for each `kild/*` worktree whose branch is merged: `git worktree remove --force <path>` + `git branch -d <branch>`. Returns the names pruned. Wire it: (a) once on engine start (`server.ts`, fire-and-forget per registered project), and (b) at the top of `GET /api/worktrees` before listing.
- **MIRROR**: the `execFile` git style in `worktree.ts`; route style in `server.ts`.
- **GOTCHA**: NEVER prune a worktree a live session is using (cross-check `sessionManager.list()` for `info.worktree`). `branch -d` (safe delete) not `-D` — refuses unmerged, a backstop. No background timer — only start + on-demand/list.
- **VALIDATE**: `bun run typecheck && bun run lint`; manual: merge a `kild/x` branch into main → hit `GET /api/worktrees` → the `x` worktree is gone, branch deleted.

### PHASE 3 — kild-owned: CLI (`--worktree` + `kild worktree …`), CLI-first

**Task 3.1: UPDATE `engine/src/cli.ts` (run `--worktree`)**
- **ACTION**: Add `worktree: { type: 'string' }` to `parseArgs` options. In `runViaEngine`, add `worktree: values.worktree` to the spawn message. In `runInProcess`, if `values.worktree`, `cwd = (await ensureWorktree(projectPath, values.worktree)).path` (mirror the worker — create-or-attach).
- **MIRROR**: `engine/src/cli.ts:16-24, 117-126, 164-176`
- **GOTCHA**: `runInProcess` is the engine-down fallback; replicate the worker's `ensureWorktree` so `--worktree` works standalone.
- **VALIDATE**: `bun run typecheck && bun run lint`; manual: `bun run cli -- run --project <p> --worktree test "touch X" --json`

**Task 3.2: UPDATE `engine/src/cli.ts` (the `kild worktree` command group — explicit management)**
- **ACTION**: Add a `worktree` group alongside `project`/`agent`/`run`: `kild worktree ls --project <p>` (lists `kild/*` worktrees, after prune-merged), `kild worktree rm <name> --project <p>` (explicit remove), `kild worktree prune --project <p>` (run the merge-prune now). Delegate to `listWorktrees`/`removeWorktree`/`pruneMergedWorktrees`. `--json` supported (CLI-first contract).
- **MIRROR**: the existing `project`/`agent` subcommand dispatch in `cli.ts`
- **VALIDATE**: `bun run typecheck && bun run lint`; manual: `bun run cli -- worktree ls --project <p> --json`

### PHASE 4 — kild-owned: cockpit

**Task 4.1: UPDATE `app/src/lib/types.ts`**
- **ACTION**: Add `branch?: string; worktreePath?: string` to both `SessionInfo` and `Session`.
- **MIRROR**: `app/src/lib/types.ts:20-42`
- **VALIDATE**: `cd app && bun run check`

**Task 4.2: UPDATE `app/src/lib/api.ts`**
- **ACTION**: Add `worktree?: string` to `SpawnOptions` (the selected worktree name; spreads into the WS spawn message). Add `listWorktrees(project)` / `removeWorktree(project, name)` / `pruneWorktrees(project)` REST helpers (GET/DELETE/POST `/api/worktrees`).
- **MIRROR**: `app/src/lib/api.ts:32, 90-92` + the existing `listProjects`/`addProject` fetch helpers.
- **VALIDATE**: `bun run check`

**Task 4.3: UPDATE `app/src/lib/components/Sidebar.svelte` + `app/src/routes/+page.svelte` (run-location selector)**
- **ACTION**: In the new-session form, add a "Run in" selector: **main checkout** (default) | **new worktree** (text input for a name) | **existing worktree** (a `Dropdown` populated from `listWorktrees(active.path)` — this is how a reviewer attaches to a coder's tree). Thread the chosen `worktree` name (undefined for main) into `startSession` → `socket.spawn({..., worktree})` → store `branch`/`worktreePath` on the local `Session` (from the next `{sessions}` reconcile, or leave null until the engine broadcasts).
- **MIRROR**: the Sidebar `showCreator` block + `Dropdown` usage + `+page.svelte:131-151`
- **GOTCHA**: default = main checkout (non-breaking). For "new worktree", auto-suggest `${agentName}-${shortId}` but let the user edit. For "existing", refresh the list when the project changes.
- **VALIDATE**: `bun run check`

**Task 4.4: UPDATE `app/src/routes/+page.svelte` (`reconcileSessions` carries branch)**
- **ACTION**: In `reconcileSessions`, copy `info.branch`/`info.worktreePath` onto the constructed `Session`.
- **MIRROR**: `app/src/routes/+page.svelte:184-206`
- **VALIDATE**: `bun run check`

**Task 4.5: UPDATE `app/src/lib/components/Topbar.svelte` (branch chip + open)**
- **ACTION**: If `activeSession.branch`, render a `⎇ {branch}` chip. Add an "open worktree" button that calls a new `openWorktree(path)` in `api.ts`.
- **MIRROR**: `app/src/lib/components/Topbar.svelte:8-19`
- **VALIDATE**: `bun run check`

**Task 4.6: UPDATE `engine/src/server.ts` + `app/src/lib/api.ts` ("open worktree")**
- **ACTION**: Engine: `POST /api/open { path }` that validates `path` starts with `worktreesRoot()` then `execFile('open'|'xdg-open', [path])`. Cockpit: `openWorktree(path)` → `fetch(POST /api/open)`. Keeps the FE pure-web (no Tauri API) and is loopback-only.
- **GOTCHA**: validate the path against `worktreesRoot()` before shelling `open` — never open arbitrary paths. (Alternative considered: Tauri `opener:allow-open-path` + re-add `@tauri-apps/api` — rejected to keep the FE pure-web.)
- **VALIDATE**: `bun run typecheck && bun run lint`; `bun run check`

**Task 4.7: CREATE the worktrees panel (explicit remove in the UI)**
- **ACTION**: A small "Worktrees" section (Sidebar, under the active project) listing `kild/*` worktrees via `listWorktrees(active.path)` with a ✕ to `removeWorktree(project, name)` and a "prune merged" action. This is the UI half of the explicit remove mechanism (the merge-prune is automatic; everything else is here).
- **MIRROR**: the Sidebar Sessions list (`Sidebar.svelte:77-86`) for row + ✕ styling.
- **GOTCHA**: refuse/confirm removing a worktree that a live session uses (the engine already cross-checks; surface its error).
- **VALIDATE**: `bun run check`

### PHASE 5 — Flue-promotable: the `worktree()` SandboxFactory

**Task 5.1: CREATE `engine/src/flue/worktree-sandbox.ts`** (mapped to Flue's real interface)
- **ACTION**: A **self-contained** factory `worktree(opts: { repo: string; branch: string; root: string }): { sandbox: SandboxFactory; cleanup: () => Promise<void> }`. `sandbox.createSessionEnv({ id })` creates a `git worktree add -B <branch> <root>/<branch>` (own minimal `execFile` git), then returns `createLocalSessionEnv({ cwd })` (mirror `local()`); `cleanup()` does `git worktree remove --force`. Caller-managed lifecycle (Flue exposes no teardown — see the mapping section). Parameterized — **no `kild/` prefix, no `kildHome()`**.
- **MIRROR**: `packages/runtime/src/node/local.ts` (`{ createSessionEnv: async () => createLocalSessionEnv(options) }`) — import `local`/`createLocalSessionEnv` from `@flue/runtime/node`.
- **GOTCHA**: `createSessionEnv` takes `{ id }` not `{ cwd }`. Must NOT import from `engine/src/kild/*` (liftability). The ~15 lines of `execFile git worktree` are **intentionally** self-contained (this is the upstream artifact).
- **VALIDATE**: `bun run typecheck && bun run lint`

**Task 5.2: CREATE `engine/src/flue/worktree-sandbox.test.ts`**
- **ACTION**: Deterministic test (no LLM, real git in a temp `git init` repo): `createSessionEnv({id:'t'})` → `env.cwd` is inside the worktree → `env.writeFile('F','x')` lands in the worktree on disk → `cleanup()` removes the worktree (`git worktree list` no longer shows it). No auto-teardown assertion (there is none).
- **MIRROR**: `engine/src/kild/events.test.ts` structure; temp dir + `git init`.
- **VALIDATE**: `bun test src/flue/worktree-sandbox.test.ts`

**Task 5.3: UPDATE `engine/src/workflows/worktree-demo.ts` (+ any `worktreeSandbox` callers)**
- **ACTION**: Import `worktree`/`worktreeSandbox` from `flue/worktree-sandbox.ts` instead of `kild/worktree.ts`.
- **VALIDATE**: `bun run typecheck`

### PHASE 6 — the Flue contribution (reference impl + issue)

**Task 6.1: Package the reference implementation**
- **ACTION**: Produce a standalone copy of `flue/worktree-sandbox.ts` + its test as a gist/branch suitable for a Flue PR (self-contained, documented, tested). Confirm it maps onto Flue's verified `SandboxFactory` shape (`createSessionEnv({id}) → SessionEnv`) and reuses `createLocalSessionEnv`.
- **VALIDATE**: `bun test` (the test passes in isolation)

**Task 6.2: Open a Flue issue/PR — two coupled proposals**
- **ACTION**: Open an issue at `github.com/withastro/flue` that surfaces a concrete gap with a reference fix:
  1. **A first-class `worktree()` git-aware sandbox** — none of Flue's sandboxes (virtual just-bash / `local` / Daytona) are git-aware, yet branch-per-agent isolation is a near-universal coding-agent need. Reference impl attached.
  2. **An optional sandbox teardown hook** — `SessionEnv` has no `dispose`/`close` and Flue never tears a sandbox down, so *any* resource-owning sandbox (worktree, ephemeral container, microVM) leaks unless the caller tracks it out-of-band. Propose `SessionEnv.dispose?(): Promise<void>` (or `SandboxFactory.teardown?(env)`) called at session end. The `worktree()` sandbox is the motivating use-case.
- **GOTCHA**: frame #2 as a discussion (it touches Flue's lifecycle model) and ship #1 as caller-managed today, so the contribution is useful even if #2 isn't adopted.
- **VALIDATE**: issue link recorded in this plan's Notes + the `flue-framework-prior-art` memory.

---

## Testing Strategy

### Unit Tests
| Test File | Cases | Validates |
|-----------|-------|-----------|
| `engine/src/kild/worktree.test.ts` | valid/invalid branches; ref + path derivation | kild policy (no git needed) |
| `engine/src/flue/worktree-sandbox.test.ts` | create → cwd-in-worktree → teardown removes | the Flue mechanism (real git, temp repo) |
| `engine/src/kild/sessions.test.ts` | branch → SessionInfo.branch/worktreePath | manager wiring (no subprocess) |

### Edge Cases
- [ ] Branch with `/` (e.g. `feat/x`) → path dashes, ref `kild/feat/x`
- [ ] Invalid branch (`--x`, `$(x)`, spaces) → `error` event / CLI non-zero exit, no git run
- [ ] Spawn with no `branch` → unchanged behavior (cwd = project dir)
- [ ] Prompt sent immediately after spawn-with-branch (worktree still creating) → buffered, not lost
- [ ] Close a worktree session → worktree dir gone, `kild/<branch>` branch still in repo
- [ ] Natural end (agent_end) → worktree KEPT for review
- [ ] `/api/open` with a path outside `worktreesRoot()` → rejected

---

## Validation Commands

### Level 1 — STATIC
```bash
cd engine && bun run typecheck && bun run lint
cd app && bun run check
```
EXPECT: exit 0.

### Level 2 — UNIT
```bash
cd engine && bun test
```
EXPECT: all pass (existing 12 + new worktree/sandbox tests).

### Level 3 — FULL / MANUAL (real agent, isolated)
```bash
cd engine && bun run dev   # engine on 127.0.0.1:4517
# register a git-repo project, then:
bun run cli -- run --project <repo> --branch demo-1 "create FILE.md with 'x', commit it" --json
git -C <repo> worktree list      # shows …/worktrees/demo-1 on kild/demo-1
git -C <repo> branch | grep kild/demo-1
# two concurrent isolated runs don't collide:
bun run cli -- run --project <repo> --branch a "touch A" &
bun run cli -- run --project <repo> --branch b "touch B" &
```
EXPECT: each agent's file lands only in its own worktree; both branches exist.

---

## Acceptance Criteria
- [ ] A session spawned with `branch` runs the agent in `…/worktrees/<branch>` on `kild/<branch>`; without `branch`, behavior is unchanged.
- [ ] Two concurrent branch-isolated sessions on one repo do not share a working tree.
- [ ] `SessionInfo`/`Session` carry `branch` + `worktreePath`; the cockpit shows the branch chip; "open worktree" reveals it.
- [ ] `kild run --branch` works both via the engine and in-process (engine-down).
- [ ] Closing a session never touches its worktree; worktrees persist. A `kild/<name>` branch merged into the default branch is auto-pruned (worktree + branch gone) on engine start / list refresh; `kild worktree rm` removes one explicitly.
- [ ] Naming the same `worktree` for two sessions attaches both to the same tree (shared); different names = split; absent = main checkout.
- [ ] **Boundary holds:** `engine/src/kild/worktree.ts` has **no** `@flue` import and is the only worktree code on the session hot path; the general mechanism lives in `engine/src/flue/worktree-sandbox.ts` with no `kild/*` imports.
- [ ] Level 1 + 2 green; manual Level 3 verified.

---

## Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Worktree creation in the worker delays the first turn / loses an early prompt | MED | MED | Stdin is OS-buffered; worktree create runs before `createAgentSession`; prompts wait. Surface failures as `error` events |
| Explicit `rm` (`--force`) discards uncommitted agent work | MED | MED | Sessions never auto-remove; merge-prune is safe (merged ⇒ integrated); only manual `rm` force-removes; (future: commit/stash-before-rm) |
| Merge-prune removes a worktree still in use, or the "default branch" is mis-detected | LOW | MED | Cross-check `sessionManager.list()` before pruning; `branch -d` (safe) not `-D`; detect default via `origin/HEAD` → `main`/`master`/current fallback |
| Branch-name injection (brain feeds LLM names) | LOW | HIGH | `assertSafeBranch` allowlist + `execFile` (no shell) — already in place; reused everywhere |
| `/api/open` opens arbitrary paths | LOW | MED | Validate path is under `worktreesRoot()` before shelling `open`; engine is loopback-only |
| Project isn't a git repo | MED | LOW | `git worktree add` fails → `error` event; branch toggle is opt-in; (future: detect + disable toggle for non-git dirs) |
| Duplication between `kild/worktree.ts` and `flue/worktree-sandbox.ts` | LOW | LOW | Intentional — the Flue module is a liftable artifact; ~15 lines, documented as deliberate |

---

## Notes

- **Why the worker (not the manager) creates the worktree:** keeps `SessionManager.spawn` synchronous (the race fix from commit `efec178` registers sessions before any await), moves the async git work into the worker (which already awaits `createAgentSession`), and lets stdin buffer early prompts. The engine still derives the deterministic `worktreePath` for `SessionInfo` so the UI updates instantly.
- **Cleanup policy:** worktree persists during the session and **after natural completion** (review/merge); removed only on **explicit close**; the `kild/<branch>` branch always persists (the unit of review for the future merge-agent team). Orphans after an engine crash are left for a future `kild worktree prune`.
- **Flue contribution framing:** kild owns *policy* (`kild/<name>`, `$KILD_HOME/worktrees`, lifecycle, cockpit); Flue gets the *mechanism* (a general `worktree()` sandbox). We contribute from production use — a stronger PR — and stay unblocked if Flue declines (we keep the adapter). Record the issue link here + in the `flue-framework-prior-art` memory.
- **Sets up:** the artifact browser (a session's worktree is browsable), branch-per-agent review, and the merge-agent team (operates on `kild/*` branches). Multi-repo (`Project.roots`) layers on top without reworking the per-repo primitive.
