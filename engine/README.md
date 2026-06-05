# kild engine

The kild engine — the agent runtime, daemon, and CLI. TypeScript on bun. It runs
`pi` coding-agent sessions in-process (coding-agent SDK, native pi auth) and exposes
them to the cockpit over HTTP + WebSocket. Flue is a complementary layer (sandbox,
deploy, orchestration workflows) and the upstream we contribute to.

## Run

```bash
bun install
bun run serve                      # HTTP + WS on :4517  (KILD_PORT to override)
bun run dev                        # serve with --watch
bun run cli -- project ls --json   # the kild CLI (secondary interface)
bun run cli -- run --model anthropic/claude-haiku-4-5 "what files are here?"
bun run cli -- run --worktree fix-auth "…"   # run isolated in a kild/fix-auth worktree
bun run cli -- worktree ls --project <p>     # list/rm/prune kild worktrees
bun run typecheck && bun run lint  # tsc + biome
```

`pi` must be on PATH and authenticated (`~/.pi/agent/auth.json` — Claude Max /
ChatGPT OAuth work natively).

## Layout

```
src/
  server.ts        HTTP (projects/agents/worktrees/open) + WS (sessions) — cockpit backend + daemon
  cli.ts           the `kild` CLI — project/agent/worktree/run
  worker.ts        per-session subprocess; ensures the worktree, then createAgentSession({cwd})
  kild/
    config.ts      default model + state dir (~/.config/kild)
    projects.ts    project registry
    agents.ts      agents from .kild/.claude/.pi convention dirs
    sessions.ts    SessionManager: coding-agent SDK sessions → UiEvent stream
    worktree.ts    [kild-owned] git worktree CRUD + ensureWorktree + merge-prune (no @flue)
    run/rooms/brain/observability/auth.ts   [Flue layer]
  flue/
    worktree-sandbox.ts   [Flue layer] worktree() SandboxFactory — the upstream contribution
  workflows/       [Flue layer] runnable Flue workflows (rooms/brain/merge/run demos)
```

Worktrees live under `$KILD_HOME/worktrees/<name>` on `kild/<name>` branches. They
**persist** — a session closing never removes one; removal is explicit (`kild worktree
rm` / cockpit ✕) or automatic only for a `kild/*` branch already merged into the default
branch (merge-prune, non-destructive: dirty/in-use trees are preserved).

`COMPARISON.md` is the decision record for choosing this engine.
```
