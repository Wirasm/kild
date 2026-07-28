---
name: kild-cli
description: |
  Drive the kild CLI: one-shot pi coding agents (`kild run`), multi-agent kilds
  (`kild new/send/log/stop`, `kild ls`), attached harnesses, and
  project/persona/worktree management.

  USE THIS SKILL to: delegate a task to a sub-agent ("use kild to…", "spawn an
  agent to…"), run/steer/observe multi-agent kilds, or manage kild projects,
  personas, and worktrees.

  Scriptable: results on stdout (`--json` for machine-readable), progress/errors
  on stderr, non-zero exit = failure. Installed globally as `kild` (bun link in
  engine/); run from any directory. Needs `pi` on PATH and authenticated.
---

# kild CLI

`kild` runs **pi** coding agents in your projects. It is the scriptable interface an
agent can drive over the Bash tool. Installed globally: run `kild <args>` from
anywhere (`bun link` from `engine/`; source `engine/src/cli.ts`).

`kild run` is **one-shot**: it starts an agent, lets it work to completion, and prints
the result. For **live, multi-agent, steerable** work, drive a **kild**
(`kild new --detach` / `ls` / `log` / `send` / `stop`) — see "Driving kilds" below.

## Vocabulary

One word per concept. See `docs/VOCABULARY.md`.

- **kild** — a git worktree and the agents working in it. The unit of isolated,
  parallel work. Also the name of the tool; "a kild" is the unit.
- **agent** — one running agent process. The only inhabitant type.
- **persona** — a markdown file that shapes what an agent is. `.pi/agents/*.md`,
  `.claude/agents/*.md`. The dir is called `agents/` by upstream convention; the
  *contents* are personas.
- **handle** — an agent's `@name`, addressable inside its kild.

## Command reference

| Command | What it does |
|---|---|
| `kild run [opts] <prompt…>` | Run one agent on a prompt to completion, print the result |
| `kild ls [--state s] [--git] [--project p]` | List kilds. **Fast by default**: identity + roster only, no git at any worktree count. `--git` adds code-state observability (branch, ahead/behind, dirty, conflicts, changed-file count, cross-kild collisions) and costs a git batch per kild. **Includes `kild/*` worktrees with no live kild** — abandoned trees show as `(orphan tree, no kild record)`, addressed by worktree name. `--state live\|orphan\|reclaimable` (`reclaimable` reads ahead/behind, so it implies `--git`) |
| `kild new <goal> --detach [opts]` | Create a kild, print its id, return. Omit `--detach` for an interactive session |
| `kild log <id> [--since <seq>]` | Read a kild's message thread. Each message carries a monotonic `seq`; `--since` is an **exclusive cursor** — pass the last seq you saw to get only what arrived after it. Works on a **stopped/archived** kild too (its log is the read-only record). Listings never carry messages |
| `kild show <id>` | One live-or-orphan kild in detail: agents, git state, full log |
| `kild send <id> <text…> --to a,b` | Send a message to named recipients in a live kild. **There is no lead and no default** — the engine never infers a recipient. `--to` is only omittable when the kild has exactly one agent, and the CLI then resolves that handle and sends it explicitly |
| `kild spawn <id> --as <handle>` | Add an agent to a live kild (`--persona`, `--model`). Errors are reported — an unknown persona or duplicate handle fails loudly |
| `kild stop <id>` | Stop a live kild by id. With `--as <handle>` stops **that one agent** and leaves the kild running |
| `kild land <id> [--execute]` | Without `--execute`: a **dry run** that touches nothing and reports what would merge and what collides. With it: merges the kild's branch into its base in the project's main checkout and prints the merge sha |
| `kild rm <id> [--force]` | Dispose of a kild's worktree. Refused when the branch carries commits its base does not have (unlanded work); uncommitted files are discarded and listed, not a refusal. **The `kild/<name>` branch always survives**, so `--force` loses no commits |
| `kild attach <id> --as <handle>` | Register an **attached** agent — a harness kild does not spawn (e.g. the Claude Code session you are in) claiming a `@handle`. Idempotent |
| `kild inbox <id> --as <handle>` | Destructively read that agent's inbox. Empty = idle. `--format claude-stop` shapes it as a Claude Code `Stop` hook |
| `kild agents` | List live agents |
| `kild project ls` | List registered projects |
| `kild project add <name> <path>` | Register a project directory (`~` is expanded) |
| `kild project rm <name>` | Remove a project |
| `kild persona ls [--project <dir>]` | List available personas (built-in `general` + convention dirs + config plugins) |
| `kild persona show <name> [--project <dir>]` | Print a persona's resolved system prompt |

There is **no `kild worktree` group**: a kild *is* a worktree, so it has one set of verbs.
`kild worktree ls` → `kild ls`, `kild worktree rm` → `kild rm`, and `kild worktree prune` →
`kild ls --state reclaimable` (then `kild rm` what you mean to reclaim — nothing is removed
behind your back). All of these take a live kild's **id** or an orphan tree's **worktree
name**; `kild ls` prints whichever applies.

Add `--json` to any command for machine-readable output on stdout.

### Flags

`--project <name|path>` · `--persona <name>` · `--model <ref>` · `--worktree <name>` ·
`--base <branch>` · `--agents a,b,c` (for `kild new`) · `--to a,b` (for `kild send`) ·
`--as <handle>` (for `attach`/`inbox`/`spawn`, and `stop` to stop one agent) ·
`--state live|orphan|reclaimable` · `--git` (both for `kild ls`) · `--since <seq>` (for
`kild log`) · `--execute` (for `kild land`) ·
`--detach` · `--force` · `--json`

## Driving kilds (multi-agent units of parallel work)

A **kild** is an isolated workspace where one or more agents collaborate. Unlike `run`
(one-shot, single agent), a kild is steerable and multi-agent:

```bash
ID=$(kild new "Build feature X" --detach --project myproj --worktree feat-x)
kild ls                       # the glance: kilds + rosters (fast — no git)
kild ls --git                 # + branch, ahead/behind, dirty, conflicts, collisions
kild log "$ID"                # the full conversation (each line prefixed by its seq)
kild log "$ID" --since 12     # only what arrived after seq 12
kild send "$ID" "use the prp-implement skill to implement plan Y"
kild send "$ID" --to reviewer "check the auth path"   # address one agent
kild stop "$ID"               # end it
```

Agents come from the project's own personas (`--agents a,b,c`; with none, one
`general` agent). Each gets kild's default prompt plus any skills the
project's config plugs in — so you can send `use the prp-X skill to …` and the agent
loads it.

A harness kild does **not** spawn — the Claude Code session you are in — can also hold a
handle: `kild attach "$ID" --as claude` registers it, and it collects mail by draining
at its own turn boundary rather than being pushed to.

## Config — plug in skills/personas, base branch, and a model catalog

`.kild/config.json` (project) and `$KILD_HOME/config.json` (global, merged; project wins):

```json
{
  "plugins": ["./prp-core"],
  "baseBranch": "dev",
  "models": {
    "openai-codex/gpt-5.6-sol": "Strongest model. Hard reasoning, orchestration, synthesis. ($$$)",
    "openai-codex/gpt-5.6-terra": "Workhorse. Research, exploration, implementation. ($$)",
    "minimax/MiniMax-M3": "Cheap/fast. Mechanical/bulk work. ($)"
  }
}
```

- `plugins` — dirs laid out like a Claude Code plugin (`agents/` + `skills/`); absolute or
  `~/…` paths load from anywhere. Also `agentPaths` / `skillPaths` for explicit dirs.
- `baseBranch` — default base for worktrees + git status.
- `models` — a `provider/model` → description catalog, appended to a **delegating**
  session's system prompt so it knows which model to pass to `spawn` per fan-out agent.

## Delegation is asynchronous

Inside a kild, `spawn` + `send` is fire-and-forget: you delegate and keep going; a
delegate's message wakes you automatically. Don't busy-wait re-asking an agent that
already replied.

**Reaching another agent requires an explicit `send`.** Your normal output is private —
narration is not delivered to anyone. If you finish delegated work without sending your
result, whoever delegated is left waiting. The engine does not chase you: reporting is
your responsibility (and your persona's), not a mechanism that nudges.

## Worktrees — every kild is one (you name them)

`--worktree <name>` runs the kild (or `kild run`) in an isolated git worktree on branch
`kild/<name>`; kild adds the `kild/` prefix, **you choose the suffix**. There is no
auto-naming — omit `--worktree` and the work happens in the project's **main checkout**
with no isolation.

Conventions when you drive kild:

- **One kild = one worktree.** Name it for the task: `--worktree fix-2247`,
  `--worktree feat-dark-mode`. Never run two independent kilds in the same tree.
- **Reuse the name to share a tree.** Two sessions naming the SAME `--worktree` attach to
  one tree (e.g. a reviewer joining a coder's kild). Different names split.
- **Base branch.** New worktrees fork from — and git status/collisions are measured
  against — the base: `--base <branch>` wins, else `.kild/config.json` `baseBranch`, else
  the checkout's current branch. On a repo whose trunk is `dev`, set `{"baseBranch":"dev"}`
  so ahead/behind and collisions reflect only this kild's work.
- **Observe & land.** `kild ls --git` shows each kild's branch, ahead/behind, dirty,
  conflicts, and cross-kild file collisions (plain `kild ls` skips git and stays fast). `kild land <id>` previews the merge without touching
  anything; `kild land <id> --execute` performs it and prints the sha (recorded on the kild,
  so the ledger names the commit).
- **Clean up.** `kild rm <id>` frees a tree. It refuses while the branch carries commits the
  base does not have — that is unlanded work, and you are told so rather than the tree
  quietly surviving. Uncommitted files are **not** a refusal (provisioning litter is not
  work): they are discarded and listed. The `kild/<name>` branch always survives, which is
  why `--force` costs no commits. `kild ls --state reclaimable` is the list `kild rm` would
  accept.
- **Stranded trees are addressable.** `kild ls` enumerates worktrees from git, so a tree
  from an earlier engine run appears as an orphan (no agents, no log) under its worktree
  name and can be landed or removed like any other kild.

## The output contract

- **stdout** — the result. Plain text by default; with `--json`, a JSON value.
- **stderr** — progress (tool activity), the model line, and stats. Ignore for parsing
  (`2>/dev/null`).
- **exit code** — `0` on success, non-zero on failure. Always check it.

## `kild run`

```
kild run [--project <name>] [--persona <name>] [--model <ref>] [--worktree <name>] <prompt…>
```

- **cwd** — defaults to the **current directory**. `--project <name>` instead runs in a
  registered project's path. Common pattern: `cd <some-dir> && kild run …`.
- **`--persona <name>`** — layer a named system prompt. Omit for the built-in `general`
  persona: no specialisation, for when no other persona fits. List options with
  `kild persona ls`.
- **`--model <ref>`** — e.g. `claude-opus-4-8`, `claude-haiku-4-5`. Omit for pi's default.
- **`--worktree <name>`** — run in an isolated `kild/<name>` worktree. Created if missing,
  **attached** if it exists. The worktree **persists** after the run.

### `--json` result shape

```json
{
  "model": "anthropic/claude-haiku-4-5",
  "text": "the agent's full reply",
  "tokens": 8092,
  "cost": 0.00176
}
```

## Examples

Run a one-shot task and capture just the answer:

```bash
kild run --json "Summarize what this repo does in two sentences." 2>/dev/null | jq -r .text
```

Delegate to a specialized persona in a registered project:

```bash
kild run --project myapp --persona planner --json \
  "Draft an implementation plan for adding OAuth login." 2>/dev/null | jq -r .text
```

Two agents on one repo in isolation, then review + clean up:

```bash
kild run --project myapp --worktree fix-auth --json "Fix the auth bug." 2>/dev/null &
kild run --project myapp --worktree add-logs --json "Add request logging." 2>/dev/null &
wait
kild ls --project myapp --git --json     # both trees, with their git state
kild land fix-auth                       # dry run: what would merge, what collides
kild land fix-auth --execute             # merge it, print the sha
kild rm fix-auth                         # free the tree (the branch stays)
```

Register a project, then list its personas:

```bash
kild project add myapp ~/projects/myapp
kild persona ls --project ~/projects/myapp --json | jq -r '.[].name'
```

## Notes

- **Personas are files.** A persona named `<name>` is a `<name>.md` file (its body is the
  system prompt) in a project's `.kild/agents/`, `.claude/agents/`, or `.pi/agents/`, or
  globally in `~/.config/kild/agents/` or `~/.claude/agents/`. The built-in `general` uses
  pi's own prompt. To add one, drop a file — kild only reads them.
- **`kild run` is one-shot.** It blocks until the agent finishes; tool progress streams to
  stderr. To steer an agent mid-flight, use a kild and `kild send`.
- **Errors are explicit.** An unknown project/persona, an unreadable persona file, or a pi
  spawn failure prints to stderr and exits non-zero — never silently degrades.
