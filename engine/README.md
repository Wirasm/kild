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
bun run typecheck && bun run lint  # tsc + biome
```

`pi` must be on PATH and authenticated (`~/.pi/agent/auth.json` — Claude Max /
ChatGPT OAuth work natively).

## Layout

```
src/
  server.ts        HTTP (projects/agents) + WS (sessions) — cockpit backend + daemon
  cli.ts           the `kild` CLI — project/agent/run
  kild/
    config.ts      default model + state dir (~/.config/kild)
    projects.ts    project registry
    agents.ts      agents from .kild/.claude/.pi convention dirs
    sessions.ts    SessionManager: coding-agent SDK sessions → UiEvent stream
    worktree.ts    git worktrees (+ a Flue local() sandbox over a worktree)
    run/rooms/brain/observability/auth.ts   [Flue layer]
  workflows/       [Flue layer] runnable Flue workflows (rooms/brain/merge/run demos)
```

`COMPARISON.md` is the decision record for choosing this engine.
```
