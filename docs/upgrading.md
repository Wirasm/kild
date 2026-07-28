# Upgrading an existing kild setup

Everything on a machine that has run kild before and needs to keep working. Ordered by how
badly it fails if you skip it.

Nothing here errors loudly. Each one either goes quiet or moves — which is why they are
written down rather than left to be discovered.

---

## 1. Move the engine's state directory

**Symptom if skipped:** every archived kild disappears from the UI and CLI. The files are
still on disk; the engine looks somewhere else now.

```sh
mv "${KILD_HOME:-$HOME/.config/kild}/rooms" "${KILD_HOME:-$HOME/.config/kild}/kilds"
```

`$KILD_HOME/rooms/<id>.json` → `$KILD_HOME/kilds/<id>.json`. No migration code ships for
this: it is a one-line `mv` on a single-operator tool, and auto-migration is the kind of
machinery this restructure exists to delete.

---

## 2. Re-point `memory.synthesis` → `hooks.onClose`

**Symptom if skipped:** synthesis silently stops running. The ledger (`LOG.md`) keeps
working; `MEMORY.md` just never updates again.

The engine no longer knows what memory synthesis *is*. It fires a declared hook on close and
hands over facts. Full detail and the exact charter to paste: **`docs/onclose-hook.md`**.

```diff
- { "memory": { "synthesis": { "model": "…", "persona": "…" } } }
+ { "hooks": { "onClose": { "agent": {
+     "model": "…", "persona": "…", "prompt": "<charter — see onclose-hook.md>"
+ } } } }
```

`memory.dir` is unchanged. Only the synthesis half moved.

---

## 3. Re-wire the Claude Code Stop hook

**Symptom if skipped:** your attached session stops receiving mail. Silently — the hook exits
0 on every failure by design, so a renamed verb does not error, it just stops delivering.

The hook script itself is fixed in-repo (`hooks/claude-stop`); what you must change is the
environment you export in the session you attach:

```diff
- export KILD_ROOM=<id>
- export KILD_PARTICIPANT=claude
+ export KILD_KILD_ID=<id>
+ export KILD_HANDLE=claude
```

And the command you run once to claim the handle:

```diff
- kild room join "$KILD_ROOM" --as "$KILD_PARTICIPANT"
+ kild attach "$KILD_KILD_ID" --as "$KILD_HANDLE"
```

The env names now match what the engine sets on the agents it spawns, so an attached agent
describes itself the same way an owned one does.

---

## 4. The built-in persona is `general`, not `default`

**Symptom if skipped:** anything naming `default` fails loudly with
`unknown persona: default`. Loud, but it will stop a script or a config mid-run.

The persona you get when you name none is now **`general`**, and it has a description so a
delegating agent can actually choose it:

> General-purpose, no specialisation. Use when no other persona fits — it can take on
> anything, given a clear goal, the outcome you want, and how you will judge it done.

`default` named its place in the system rather than what it is; "spawn `default`" tells an
agent nothing about what it would get. There is **no alias** — per the no-shims rule, the old
name is gone.

Change anywhere you name it explicitly:

```diff
- kild run --persona default …
+ kild run --persona general …

- {"agents":[{"handle":"coder","persona":"default"}]}
+ {"agents":[{"handle":"coder","persona":"general"}]}

- {"hooks":{"onClose":{"agent":{"persona":"default", …}}}}
+ {"hooks":{"onClose":{"agent":{"persona":"general", …}}}}
```

Omitting `persona` entirely still works and still means the built-in — only the explicit
spelling changed. A persona file named `general.md` does **not** shadow the built-in.

---

## 5. Relearn the CLI verbs

**Symptom if skipped:** commands fail loudly with a usage line. The least dangerous item here.

| Was | Now |
|---|---|
| `kild rooms` | `kild ls` |
| `kild room open <goal>` | `kild new <goal>` |
| `kild room post <id> <text>` | `kild send <id> --to <handle> <text>` |
| `kild room log <id>` · `kild room show <id>` | `kild log <id>` · `kild show <id>` |
| `kild room close <id>` | `kild stop <id>` |
| `kild room join <id> --as <h>` | `kild attach <id> --as <h>` |
| `kild room drain <id> --as <h>` | `kild inbox <id> --as <h>` |
| `kild operator …` | *(deleted — your agent CLI drives the API directly)* |
| `kild sessions` | `kild agents` |
| `kild agent ls` · `kild agent show` | `kild persona ls` · `kild persona show` |
| `--participants a,b` | `--agents a,b` |
| `--agent <name>` | `--persona <name>` |

`kild send` now **requires `--to`** — the engine never infers a recipient. The CLI will
resolve it for you when a kild has exactly one agent, but that is the client being
convenient, not the engine defaulting.

Full reference: `.claude/skills/kild-cli/SKILL.md`.

---

## 6. Reclaim stranded worktrees

**Symptom if skipped:** disk fills. Measured on one machine: 116 kild worktrees, **zero**
reclaimable by `prune`.

`prune` only removes trees whose branch already merged *and* which are clean, so abandoned
work was structurally permanent. Worse, any untracked file counted as dirty — 27 of 27 agent
worktrees on that machine were dirty from provisioning litter written before any agent ran.

`docs/worktree-disposal.md` has the full diagnosis. The immediate fix, from the writer's
side: **whatever tooling writes files into every worktree should gitignore them.**
`git status --porcelain` already respects `.gitignore`, so ignored litter stops registering
as dirty and normal cleanup starts working again.

For trees already stranded, `kild rm <name> --force` bypasses the authored-commits refusal —
there is no `kild worktree` group any more, the worktree family folded into the kild
collection. It LISTS what it discarded (and says so explicitly when git could not produce
that list), and the `kild/<name>` branch always survives, so force costs no commits.

---

## Config keys: what changed

| Key | Status |
|---|---|
| `plugins` · `agentPaths` · `skillPaths` | unchanged |
| `baseBranch` · `models` | unchanged |
| `memory.dir` | unchanged — still the ledger/memory directory |
| `memory.synthesis` | **removed** → `hooks.onClose` |
| `hooks.onClose` | **new** — see `docs/onclose-hook.md` |

## Environment variables

| Var | Status |
|---|---|
| `KILD_HOME` · `KILD_PORT` · `KILD_ENGINE` | unchanged |
| `KILD_ROOM` → `KILD_KILD_ID` | renamed (Stop-hook wiring) |
| `KILD_PARTICIPANT` → `KILD_HANDLE` | renamed (Stop-hook wiring) |
| `KILD_OPERATOR` | **removed** — the operator tier is gone |
| `KILD_ROLE=worker` → `KILD_ROLE=agent` | renamed (internal; only matters if you invoke the worker directly) |
| `KILD_SESSION_ID` → `KILD_AGENT_ID` | renamed (internal — the engine sets it on every agent it spawns). `session` means pi's conversation and nothing else, and this is the engine's own id for an agent's process. The REST field it is presented as renamed with it: `sessionId` → `agentId`. |

## For helm and other API clients

Do not port yet. See `docs/helm-migration.md` — the un-pin point is after the API reshape,
not after the rename. Porting to an intermediate state means doing the work more than once.
