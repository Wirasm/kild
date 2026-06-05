---
name: kild-cli
description: |
  Drive the kild CLI to run pi coding agents as one-shot tasks and to manage the
  projects and agents they use. `kild run` spawns an AI coding agent (pi), drives
  it to completion in a directory, and returns its result — text, tools used, and
  token/cost stats — as JSON.

  USE THIS SKILL when you need to:
  - Delegate a self-contained task to a sub-agent: "use kild to ...", "spawn an
    agent to ...", "run a kild agent", "have a kild agent do ..."
  - Run a task with a specialized agent (a planner, reviewer, ...) in a project
  - Manage kild projects: "list/add/remove kild projects"
  - List or inspect kild agents (named system prompts)

  kild is CLI-first and scriptable. Read commands print to stdout (pass `--json`
  for machine-readable output); progress and errors go to stderr; a non-zero exit
  code means failure. Parse stdout, check the exit code.

  Prerequisite: `pi` must be on PATH and authenticated. This skill drives the
  kild engine CLI: from `engine/`, run `bun run cli -- <args>` (or the linked
  `kild` bin). It does NOT manage the older worktree/session `kild` — that is the
  separate `kild` skill.
---

# kild CLI

`kild` runs **pi** coding agents in your projects. It is the scriptable interface
an agent can drive over the Bash tool — the engine CLI (`engine/src/cli.ts`, run
with `bun run cli -- <args>`). The CLI's `run` is **one-shot**: it starts an agent,
lets it work to completion, and prints the result. (Live, steerable sessions are
driven from the cockpit UI over the engine's WebSocket, not the CLI.)

## Command reference

| Command | What it does |
|---|---|
| `kild run [opts] <prompt…>` | Run an agent on a prompt to completion, print the result |
| `kild project ls` | List registered projects |
| `kild project add <name> <path>` | Register a project directory (`~` is expanded) |
| `kild project rm <name>` | Remove a project |
| `kild agent ls [--project <dir>]` | List available agents (built-in `default` + convention dirs) |
| `kild agent show <name> [--project <dir>]` | Print an agent's resolved system prompt |
| `kild worktree ls --project <p>` | List the project's `kild/*` worktrees |
| `kild worktree rm <name> --project <p>` | Remove a worktree (frees disk; the `kild/<name>` branch persists) |
| `kild worktree prune --project <p>` | Remove **and `-d`-delete the branch of** each `kild/*` worktree merged into the default branch (clean trees only; dirty/in-use ones are kept) |

Add `--json` to any command for machine-readable output on stdout.

## The output contract

- **stdout** — the result. Plain text by default; with `--json`, a JSON value.
- **stderr** — progress (tool activity), the model line, and stats. Ignore it for
  parsing (`2>/dev/null`).
- **exit code** — `0` on success, non-zero on failure (the error message is on
  stderr). Always check it.

## `kild run`

```
kild run [--project <name>] [--agent <name>] [--model <pattern>] [--worktree <name>] <prompt…>
```

- **cwd** — defaults to the **current directory** (the agent works wherever you
  are). `--project <name>` instead runs in a registered project's path. So the
  common pattern is `cd <some-dir> && kild run …`.
- **`--agent <name>`** — layer a named system prompt on pi's default (a specialized
  role). Omit for the plain `default` agent. List options with `kild agent ls`.
- **`--model <pattern>`** — e.g. `claude-opus-4-8`, `claude-haiku-4-5`. Omit to use
  pi's configured default.
- **`--worktree <name>`** — run the agent in an isolated `kild/<name>` **git
  worktree** instead of the project dir, so concurrent agents on one repo don't
  collide. Created if missing, **attached** if it already exists (two runs with the
  same name share a tree). Omit to run in the main checkout. The worktree **persists**
  after the run — review/merge `kild/<name>`, then `kild worktree rm <name>`.

### `--json` result shape (`RunOutcome`)

```json
{
  "model": "anthropic / claude-haiku-4-5",
  "text": "the agent's full reply",
  "tools": [{ "name": "bash", "ok": true }],
  "tokens": 8092,
  "cost": 0.00176,
  "context_pct": 4.0
}
```

`model`, `tokens`, `cost`, and `context_pct` may be `null` if pi did not report
them. `text` is the concatenated assistant reply. `tools` lists each tool call in
order with whether it succeeded.

## Examples

Run a one-shot task and capture just the answer:

```bash
kild run --json "Summarize what this repo does in two sentences." 2>/dev/null \
  | jq -r .text
```

Delegate to a specialized agent in a registered project:

```bash
kild run --project myapp --agent planner --json \
  "Draft an implementation plan for adding OAuth login." 2>/dev/null | jq -r .text
```

Run in a specific directory (e.g. a worktree) with a fast model:

```bash
cd /path/to/worktree
kild run --model claude-haiku-4-5 "Run the tests and report failures."
```

Run two agents on one repo in isolation, then review + clean up their branches:

```bash
kild run --project myapp --worktree fix-auth --json "Fix the auth bug." 2>/dev/null &
kild run --project myapp --worktree add-logs --json "Add request logging." 2>/dev/null &
wait
kild worktree ls --project myapp --json   # each on its own kild/<name>
# after reviewing/merging kild/fix-auth:
kild worktree rm fix-auth --project myapp
```

Register a project, then list agents available to it:

```bash
kild project add myapp ~/projects/myapp
kild agent ls --project ~/projects/myapp --json | jq -r '.[].name'
```

## Notes

- **Agents are files.** An agent named `<name>` is a `<name>.md` file (its body is
  the system prompt) found in a project's `.kild/agents/`, `.claude/agents/`, or
  `.pi/agents/`, or globally in `~/.config/kild/agents/` or `~/.claude/agents/`.
  The built-in `default` agent uses pi's own prompt. To add an agent, drop a file —
  kild only reads them.
- **One-shot only.** `kild run` blocks until the agent finishes. For a long task,
  expect it to take a while; tool progress streams to stderr so you can see it
  working. There is no way yet to attach to or steer a running agent from the CLI.
- **Errors are explicit.** An unknown project/agent, an unreadable agent file, or a
  pi spawn failure prints to stderr and exits non-zero — never silently degrades.
