# AGENTS.md — kild

kild is an orchestration engine for agent teams: **kilds** (a git worktree and the agents
working in it — the unit of isolated, parallel work), the agents themselves, and the
observability around them. It is engine + CLI + pi extension only — there is no UI in this
repo; UI clients (helm) consume the REST/WS API on `localhost:4517`.

One inhabitant type: the **agent**. No ranks, no lead, no operator, no privileged human.

## What wins, in order

1. **Simplicity.** The smallest mechanism that does the job. A rule that only applies at one
   size, a default the engine infers, an abstraction with one implementation — all cost more
   than they pay.
2. **Dogfood.** Run it before believing it. Green unit tests are not a working system;
   `scripts/e2e.sh` exists because four real bugs were invisible to 313 passing tests.
3. **Apply the learnings.** When something bites, fix the class, not the instance.

## Boundaries (violations are bugs)

- **Only the engine knows pi exists.** pi shapes are translated into kild domain types in one
  place; the CLI and API clients never see them.
- **kild ships mechanism, never intelligence.** No personas, no process prompts in code —
  personas are data (`.pi/agents/`, config `plugins:`); the only shipped prompt is the
  generic mechanism prompt.
- **kild knows nothing about prp** (or any intelligence layer). Integration happens via
  generic config paths (`plugins`, `memory.dir`, `hooks.onClose`) only.
- **Never read intent out of an agent's prose.** No regex over a message body to decide what
  the engine does. Addressing did that and the "who is this for?" bug hid in it for months;
  the decisions ledger did it and parsed a protocol out of English. Both are gone. A sender
  names its recipients as data; a hook is declared in config; anything an agent means, it
  states through a tool call.
- **Don't enforce what you can't enforce.** The engine is loopback and single-operator: any
  local process can read any credential it holds. Attribution is worth building; authorization
  ceremony on top of it is not. Solve the problems that exist.
- Vocabulary: `docs/VOCABULARY.md` — one word per concept, and the dead words are listed.

## How to work here

- Layout: `engine/src/` (`server.ts`, `cli.ts`, `agent.ts`, `kild/` domain slices),
  `engine/scripts/e2e.sh`, `pi-extension/`, `docs/`.
- The engine is usually RUNNING from this checkout, and may be hosting the very run you are
  part of. **Never shut it down, and never bind port 4517.** Starting your own engine on a
  different port is fine. Do code work in a git worktree on a branch, PR to `main`
  (`bun install` first — a fresh worktree has no `node_modules`, so the gates cannot run).
- Gates: `bun test`, `bun run typecheck`, `bun run lint` from `engine/` — all green before
  any PR. `./scripts/e2e.sh` for anything touching the HTTP surface.
- **Never delete an existing test to make a gate green.** If a test is genuinely obsolete,
  say which and why in the commit message.
- **A malformed request is a 400, never a 500.** The server does not report its own failure
  for a client's mistake.
- Conventional commits, written as a human — no AI attribution, no Co-Authored-By.
- Vertical slices: a change owns its types and logic; extend by adding a slice, never by
  growing a god-module. No shims, no backwards-compat aliases — the only consumers are our
  own CLI and UI, so rename outright and port them.

## Docs

`docs/VOCABULARY.md` (naming) · `docs/upgrading.md` (what an existing setup must change) ·
`docs/helm-migration.md` (the API, for clients) · `docs/onclose-hook.md` (the lifecycle seam) ·
`docs/attached-agents.md` (the harness-kild-does-not-own transport) ·
`docs/api-surface.md` and `docs/DEMOLITION.md` (why the shape is what it is)
