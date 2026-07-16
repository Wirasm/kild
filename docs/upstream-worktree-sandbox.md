# Upstream proposal: `worktree()` SandboxFactory

## Proposal

Add a `worktree({ repo, branch, root })` factory to Flue's Node sandbox package. Its
`SandboxFactory.createSessionEnv()` removes any prior checkout at
`<root>/<branch-with-slashes-replaced>`, runs `git -C <repo> worktree add -B <branch>`
to materialize it, and delegates the resulting session environment to
`local({ cwd: worktreePath })`. The factory returns `{ sandbox, cleanup }`; callers
invoke `cleanup()` to run `git worktree remove --force`, because Flue currently has
no session-environment teardown hook.

## Why Flue owns it

This is a general SandboxFactory mechanism, not kild policy: it has no kild imports,
branch naming convention, project registry, or lifecycle ownership. It lets any Flue
workflow run an agent in an isolated Git checkout while preserving Flue's standard
local filesystem and shell environment.

## Coverage

The accompanying test creates a temporary committed Git repository, verifies that
`createSessionEnv()` materializes the worktree, that its cwd and writes resolve
inside it, that Git lists it while active, and that caller-managed cleanup removes
it.

## Files to lift

- `engine/src/flue/worktree-sandbox.ts`
- `engine/src/flue/worktree-sandbox.test.ts`

Place the implementation beside Flue's Node sandbox factories and the test in that
package's corresponding test location; no other kild files are required.
