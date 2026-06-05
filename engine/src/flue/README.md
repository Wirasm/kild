# Flue-promotable mechanisms

Self-contained modules built in kild that are candidates to graduate upstream into
[`@flue/runtime`](https://github.com/withastro/flue). Each is intentionally free of
`kild/*` imports so it can be lifted verbatim.

## `worktree-sandbox.ts` — a git-worktree `SandboxFactory`

A `worktree({ repo, branch, root })` factory returning `{ sandbox, cleanup }`. Its
`sandbox.createSessionEnv({ id })` materializes a `git worktree add -B <branch>` and
delegates the fs/exec env to Flue's `local({ cwd })`; `cleanup()` removes the
worktree. Verified against Flue's real interface (`packages/runtime/src/types.ts`):

- `SandboxFactory.createSessionEnv(options: { id: string }): Promise<SessionEnv>` ✓
- `SessionEnv` has **no** `dispose`/`close`/`teardown`, and Flue never tears a
  sandbox down — so lifecycle is **caller-managed**, hence the returned `cleanup()`.
- Mirrors `local()` (`packages/runtime/src/node/local.ts`). Note: this Flue version
  (`^0.9.2`) exports `local` but **not** `createLocalSessionEnv`, so we delegate
  through `local({ cwd }).createSessionEnv({ id })` rather than calling the lower-level
  helper the plan assumed.

### The upstream RFC (two coupled proposals) — [withastro/flue#207](https://github.com/withastro/flue/discussions/207)

1. **A first-class `worktree()` git-aware sandbox.** None of Flue's sandboxes
   (virtual just-bash / `local` / Daytona) are git-aware, yet branch-per-agent
   isolation is a near-universal coding-agent need. `worktree-sandbox.ts` is the
   reference impl, shippable today as caller-managed.
2. **An optional sandbox teardown hook** (discussion). `SessionEnv` has no
   `dispose`/`close` and Flue never tears a sandbox down, so *any* resource-owning
   sandbox (worktree, ephemeral container, microVM) leaks unless the caller tracks it
   out-of-band. Propose `SessionEnv.dispose?(): Promise<void>` (or
   `SandboxFactory.teardown?(env)`) called at session end; the `worktree()` sandbox is
   the motivating use-case. Frame as a discussion so #1 lands even if #2 isn't adopted.

Opened 2026-06-05 as a Feature Request discussion (RFC): https://github.com/withastro/flue/discussions/207
