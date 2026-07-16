# kild Direction

The maintainer agents consult this when triaging issues/PRs — what kild IS, what
it is NOT, and which contributions fit. **Committed and shared.** Cite a clause
when recommending a decline/close, e.g. `direction.md §opinionated-not-agnostic`.
Keep entries to a line or two — this is for fast lookup during triage, not a manifesto.

---

## What kild IS

- **An opinionated orchestrator for pi/Flue agent teams.** One operator directs
  many isolated coding agents — each isolated, all visible, the work landed. (§opinionated)
- **A single-developer tool.** One operator, many agents. No multi-tenant
  complexity, accounts, billing, or SaaS scaffolding. (§single-developer)
- **Two halves, one boundary.** The engine (TypeScript/bun) owns orchestration;
  pi owns cognition (models, sessions, compaction, tools, auth). Only the engine
  touches pi; pi shapes are translated to kild domain types at that boundary. (§pi-boundary)
- **Prompts are data, not code.** Agent personalities live in `.pi`/`.claude`/`.kild`
  dirs, authored and edited by humans *and* agents — never baked into the codebase. (§prompts-are-data)
- **Committed to pi + Flue.** Depth over breadth: the deepest pi-native
  orchestration and Flue-backed isolation, contributing upstream. (§pi-flue-bet)
- **Engineered tight.** Strict TypeScript (no `any` in prod paths), KISS/YAGNI,
  fail-fast (errors surfaced, never swallowed), no shims/backwards-compat. (§engineering)
- **CLI-first.** Every capability is reachable and testable via the `kild` CLI. (§cli-first)
- **Green checks = done.** `bun run typecheck`, `bun run lint`, and the FE's
  `bun run check` all pass before anything lands. (§green-checks)

## What kild is NOT

- **Not agent-agnostic.** It bets on pi/Flue; PRs that abstract over arbitrary
  agent backends conflict with the moat. (§opinionated-not-agnostic)
- **Not multi-tenant, not a hosted service.** No accounts, roles, billing, or
  proprietary backend deps. (§single-developer)
- **Not a prompt framework.** kild ships orchestration mechanism and at most a
  default system prompt; it never bakes in agent personalities. (§prompts-are-data)
- **Not a reimplementation of pi.** Providers, sessions, context compaction,
  tool-calling, and auth are pi's job. (§pi-boundary)
- **No shims / backwards-compat.** Greenfield, single dev — one name, one type,
  one location. If something is unused, delete it. (§no-shims)

## Open questions (no stance yet)

- (none yet — add as a triage decision forces a call)

## How to evolve this

Add an IS / IS-NOT line when a triage decision forces a direction call; move open
questions into IS/IS-NOT once decided. Reference the clause in the comment when
declining.
