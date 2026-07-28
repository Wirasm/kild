# kild engine

The kild engine — the agent runtime, daemon, and CLI. TypeScript on bun. It runs
`pi` coding-agent sessions in-process (coding-agent SDK, native pi auth) and exposes
them over HTTP + WebSocket — the API contract external UI clients (e.g.
[helm](https://github.com/Wirasm/helm)) consume.

## Run

```bash
bun install
bun run serve                      # HTTP + WS on :4517  (KILD_PORT to override)
bun run dev                        # serve with --watch
bun run cli -- project ls --json   # the kild CLI (secondary interface)
bun run cli -- run --model anthropic/claude-haiku-4-5 "what files are here?"
bun run cli -- run --worktree fix-auth "…"   # run isolated in a kild/fix-auth worktree
bun run cli -- ls --state orphan             # kilds + kild/* trees git knows about
bun run cli -- land <id> --execute           # merge a kild's branch into its base
bun run cli -- rm <id>                       # free a kild's worktree (branch survives)
bun run typecheck && bun run lint  # tsc + biome
```

`pi` must be on PATH and authenticated (`~/.pi/agent/auth.json` — Claude Max /
ChatGPT OAuth work natively).

## Layout

```
src/
  server.ts        HTTP (projects/kilds/agents/open) + WS (kilds) — API server + daemon
  cli.ts           the `kild` CLI — kild/project/persona/run
  agent.ts         per-agent subprocess; ensures the worktree, then createAgentSession({cwd})
  kild/
    config.ts      kild config (plugins, base branch, memory) + state dir (~/.config/kild)
    projects.ts    project registry
    personas.ts    personas from .kild/.claude/.pi convention dirs
    agent-manager.ts  one subprocess per agent: coding-agent SDK sessions → UiEvent stream
    worktree.ts    git worktree CRUD + ensureWorktree + merge-prune
    kild-trees.ts  the kild inventory as GIT knows it (so a record-less tree is addressable)
    kild-disposal.ts  the disposal guard: authored commits refuse, litter does not
    kild-land.ts   land a kild's branch into its base (dry run + execute)
```

Worktrees live under `$KILD_HOME/worktrees/<name>` on `kild/<name>` branches. They
**persist** — a session closing never removes one; removal is explicit (`kild rm` /
`DELETE /api/kilds/:id`) or automatic only for a `kild/*` branch already merged into the
default branch (merge-prune on startup). Explicit disposal is guarded on **authored
commits**, not on dirt: a branch with commits base lacks is refused (`--force` overrides —
the branch, and every commit on it, always survives), while uncommitted files are discarded
and listed.
