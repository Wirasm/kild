# Implementation Report

**Plan**: `.claude/PRPs/plans/worktree-per-session.plan.md`
**Branch**: `feature/worktree-per-session`
**Date**: 2026-06-05
**Status**: COMPLETE — passed code review with fixes (see *Post-Review Fixes* below);
one external follow-up: open the Flue upstream issue

---

## Summary

Each agent session can now optionally run in its own isolated `kild/<name>` git
worktree instead of the raw project directory, so concurrent agents on one repo no
longer trample each other. A session carries an optional `worktree` **name**: absent →
main checkout (planners, brain, review-of-main); present → the worker
**create-or-attaches** the `kild/<name>` worktree (same name ⇒ shared tree, different
name ⇒ split). Worktrees **persist** — a session closing never removes one; removal is
explicit (`kild worktree rm` / UI) or automatic only via merge-prune (a `kild/*` branch
merged into the default branch is pruned on engine start and on each list refresh).

The work held the planned boundary: **kild owns policy** (`engine/src/kild/worktree.ts`,
hot-path-safe, **no `@flue` import**), and the **general mechanism** is a self-contained
`worktree()` SandboxFactory in `engine/src/flue/worktree-sandbox.ts` (no `kild/*`
imports, liftable into Flue).

---

## Assessment vs Reality

| Metric     | Predicted | Actual | Reasoning |
| ---------- | --------- | ------ | --------- |
| Complexity | MEDIUM    | MEDIUM | Matched. Touched 14 files + 3 new; no surprises in the session/worker wiring. |
| Confidence | High      | High   | The race-free design (worker ensures, manager derives synchronously) held exactly as planned; verified end-to-end with a real agent. |

**Deviations (see below) were minor and driven by the real Flue API surface.**

---

## Tasks Completed

| Phase | Task | File(s) | Status |
| --- | --- | --- | --- |
| 1 | De-Flue worktree.ts; add `worktreeRef`/`worktreePath`/`ensureWorktree`/`pruneMergedWorktrees`; export `assertSafeBranch`/`worktreesRoot` | `engine/src/kild/worktree.ts` | ✅ |
| 1 | Deterministic helper tests | `engine/src/kild/worktree.test.ts` | ✅ |
| 2 | `SpawnRequest.worktree`, `SessionInfo.branch/worktreePath/worktree`, `KILD_WORKTREE` env, sync derivation, no-remove-on-stop | `engine/src/kild/sessions.ts` | ✅ |
| 2 | Worker ensures the worktree before `createAgentSession` | `engine/src/worker.ts` | ✅ |
| 2 | Validate `worktree`; `/api/worktrees` GET/DELETE + `/prune`; prune-on-start | `engine/src/server.ts` | ✅ |
| 2 | Derivation unit tests | `engine/src/kild/sessions.test.ts` | ✅ |
| 3 | `run --worktree` (engine + in-process); `kild worktree ls/rm/prune` group | `engine/src/cli.ts` | ✅ |
| 4 | `branch`/`worktreePath` FE types | `app/src/lib/types.ts` | ✅ |
| 4 | `SpawnOptions.worktree`; worktree REST helpers + `openWorktree` | `app/src/lib/api.ts` | ✅ |
| 4 | "Run in" selector (main / new / existing) | `app/src/lib/components/SessionModal.svelte`, `app/src/routes/+page.svelte` | ✅ |
| 4 | `reconcileSessions` carries + patches branch | `app/src/routes/+page.svelte` | ✅ |
| 4 | Branch chip + open action | `app/src/lib/components/Topbar.svelte` | ✅ |
| 4 | `POST /api/open` (scoped to worktree root) + `openWorktree` | `engine/src/server.ts`, `app/src/lib/api.ts` | ✅ |
| 4 | Worktrees panel (list + ✕ remove + prune) | `app/src/lib/components/Sidebar.svelte` | ✅ |
| 5 | `worktree()` SandboxFactory `{sandbox, cleanup}` | `engine/src/flue/worktree-sandbox.ts` | ✅ |
| 5 | Real-git temp-repo test | `engine/src/flue/worktree-sandbox.test.ts` | ✅ |
| 5 | Demo imports the Flue factory | `engine/src/workflows/worktree-demo.ts` | ✅ |
| 6 | Reference impl + issue draft | `engine/src/flue/README.md` | ✅ |
| 6 | Open the Flue issue at withastro/flue | — | ⏭️ external follow-up |
| docs | `--worktree` + `kild worktree` group | `.claude/skills/kild-cli/SKILL.md` | ✅ |
| docs | Worktree slice + boundary + protocol | `CLAUDE.md` | ✅ |

---

## Validation Results

| Check | Result | Details |
| ----- | ------ | ------- |
| Type check (engine) | ✅ | `tsc --noEmit`, 0 errors |
| Lint (engine) | ✅ | biome, 0 errors |
| Unit tests (engine) | ✅ | 21 passed (12 pre-existing + 9 new), 0 failed |
| Build (engine) | ✅ | `bun build --compile` → `dist/kild-engine` |
| Check (app) | ✅ | svelte-check 0 errors (1 pre-existing node-typedef warning) |
| Integration (REST) | ✅ | `/api/worktrees` list/delete/prune + `/api/open` (200 valid, **403** for `/etc`) |
| Integration (Level 3, real agent) | ✅ | Isolated run created `FILE.md` in `…/worktrees/demo-1` on `kild/demo-1`; absent from main repo; worktree persisted after close |

---

## Files Changed

**Created (5):** `engine/src/kild/worktree.test.ts`, `engine/src/kild/sessions.test.ts`,
`engine/src/flue/worktree-sandbox.ts`, `engine/src/flue/worktree-sandbox.test.ts`,
`engine/src/flue/README.md`.

**Updated (14):** `engine/src/kild/worktree.ts`, `engine/src/kild/sessions.ts`,
`engine/src/worker.ts`, `engine/src/server.ts`, `engine/src/cli.ts`,
`engine/src/workflows/worktree-demo.ts`, `app/src/lib/types.ts`, `app/src/lib/api.ts`,
`app/src/routes/+page.svelte`, `app/src/lib/components/SessionModal.svelte`,
`app/src/lib/components/Sidebar.svelte`, `app/src/lib/components/Topbar.svelte`,
`.claude/skills/kild-cli/SKILL.md`, `CLAUDE.md`.

---

## Deviations from Plan

1. **`createLocalSessionEnv` is not exported** by `@flue/runtime/node@0.9.2` — only
   `local`. The plan's Task 5.1 assumed it was importable. The Flue `worktree()`
   factory therefore delegates through `local({ cwd }).createSessionEnv({ id })`
   instead of calling the lower-level helper. Same effect, matches the real API, still
   a faithful mirror of `local()`.

2. **CLI flag is `--worktree`, not `--branch`.** The plan was internally inconsistent
   (metadata/UX table said `--branch`; the authoritative Task 3.1 + its manual-test
   command said `--worktree`). Chose `--worktree` to match the wire field (`worktree`),
   the env var (`KILD_WORKTREE`), and the "worktree is a selectable named resource"
   model — one name, no alias (per the project's no-shims rule).

3. **The Flue module exports `worktree()` (the richer API), not `worktreeSandbox(wt)`.**
   The plan's seed snippet showed `worktreeSandbox` moving to `flue/`, but Task 5.1
   specifies the parameterized `worktree({repo,branch,root})`. The demo
   (`worktree-demo.ts`) was refactored onto `worktree()` accordingly.

4. **Phase 5 was brought forward to immediately after Phase 1** (not done last). Deleting
   `worktreeSandbox` from `kild/worktree.ts` broke `worktree-demo.ts`'s import; creating
   the `flue/` module + repointing the demo in the same step kept the build green rather
   than accumulating a broken intermediate state.

5. **Task 6.2 (opening the Flue issue) is an external action** — out of scope for an
   autonomous run (no creds for withastro/flue, and the plan's "NOT Building" lists PR
   acceptance as out of our control). The reference impl + a ready-to-post issue draft
   live in `engine/src/flue/README.md`; opening it is a manual follow-up.

---

## Issues Encountered

- macOS lacks `timeout(1)` — the first Level 3 run aborted on `command not found`;
  re-ran without it (the CLI's `run` resolves on `agent_end` anyway). No code impact.
- `biome format` does not apply the `organizeImports` assist; needed
  `biome check --write` for import sorting. No code impact.

---

## Tests Written

| Test File | Test Cases |
| --------- | ---------- |
| `engine/src/kild/worktree.test.ts` | safe/unsafe branch names; `worktreeRef`; `worktreePath` (slash→dash); reject-before-I/O |
| `engine/src/kild/sessions.test.ts` | name → `kild/` ref + path; slashed name; unsafe name throws |
| `engine/src/flue/worktree-sandbox.test.ts` | real temp git repo: `createSessionEnv` cwd is inside the worktree, `writeFile` lands on disk, `cleanup()` removes it |

---

## Post-Review Fixes

A code review (verdict: NEEDS FIXES — 1 Critical, 6 Important, suggestions) was run on
the diff. All Critical + Important items addressed; suggestions taken unless declined
with reason.

**🔴 Critical — fixed:** `pruneMergedWorktrees` used `git worktree remove --force`,
which destroys uncommitted/untracked work on a merged branch (tip == base ⇒ "merged",
then force-removed) — and it ran on every `GET /api/worktrees` and `kild worktree ls`.
Fix: prune now uses **non-force** `git worktree remove` (git refuses dirty/untracked
trees → work preserved); the branch is `-d`-deleted only after the tree is removed;
`pruned` is reported only on actual removal. `--force` stays in the user-initiated
`removeWorktree`/`createWorktree`. Covered by a new test.

**🟠 Important — fixed:**
1. *CLI prune/ls ignored the in-use guard* (separate process, empty keep set). Now the
   `worktree` group **routes through the engine when it's up** (its endpoints skip
   in-use worktrees); when the engine is down, no live session can exist, so direct
   operation is safe. `kild worktree ls` is **read-only** (no implicit prune).
2. *`ensureWorktree` could attach to a stale/non-worktree dir.* Now it attaches only if
   the dir has a `.git` pointer; a leftover dir throws (fail-fast, no silent
   non-isolated cwd). Also: re-creating a removed worktree now **checks out the existing
   branch** (never `-B`), preserving its commits.
3. *Swallowed errors.* prune no longer blanket-catches (remove-failure ⇒ preserve;
   branch-d failure ⇒ logged); the Flue `cleanup()` now propagates; startup prune logs.
4. *Worker stdin `JSON.parse` was uncaught* → wrapped (a malformed line is skipped, not
   a worker crash).
5. *Tests for the dangerous functions* — added `worktree.git.test.ts` (real git):
   prune-merged / skip-unmerged / keep-set / never-default / **preserve-dirty**, and
   ensureWorktree attach-not-reset / preserve-branch-on-recreate / throw-on-stale.

**🟡 Suggestions — taken:** `Worktree.name` (engine-supplied; removed the `kild/`-strip
regex in ~4 FE/engine spots) + a `worktreeName(branch)` helper; `startSession`
nested ternary → `selectedWorktree()` helper; `worktree ls`'s `await…then()` → straight
statements; per-repo prune coalescing guard (avoids git index-lock races on
startup/list/CLI); `openSessionModal` now surfaces a `listWorktrees` failure as a banner.

**Declined (with reason):**
- *Grouping `SessionInfo.{worktree,branch,worktreePath}` into a sub-object* — the partial
  state (branch set instantly on spawn, `worktreePath` arriving via the engine
  broadcast) is **intentional** for the instant chip; grouping would make that transient
  harder, not safer.
- *Aligning the Flue demo's `<root>/kild-<name>` vs the session path's `<root>/<name>`* —
  the reviewer confirmed it's a naming difference, not a live bug; aligning would blur
  the kild/Flue boundary (the Flue module is deliberately ignorant of kild's `kild/`
  policy).
- *Per-launch auth token* — YAGNI for a loopback single-user tool (reviewer: "later").

**Docs updated:** `engine/README.md` (stale layout/endpoints/CLI; worker + flue/
entries; persist + non-destructive prune note); `SKILL.md` (`prune` deletes the branch
too; `ls` is read-only).

**Re-validation:** engine typecheck ✓, biome ✓, **29 tests pass** (was 21; +8 git
safety tests), app check ✓. Integration re-verified: `ls` read-only, `prune` deletes
the branch and **preserves a dirty merged worktree**, CLI engine-up routing (ls/rm).

---

## Next Steps

- [ ] Review the implementation (esp. the 5 deviations + the post-review fixes above)
- [ ] Open the Flue issue at withastro/flue (draft in `engine/src/flue/README.md`);
      record the link there + in the `flue-framework-prior-art` memory
- [ ] Create PR: `gh pr create` / `/prp-pr`
- [ ] Merge when approved
