# `hooks.onClose` — and migrating off `memory.synthesis`

The engine no longer knows what memory synthesis is. It fires a **declared hook** on kild
close, carrying only facts it holds. What to do with those facts is the declarer's business.

**If you have `memory.synthesis` in a config, it is no longer read.** Nothing errors — the
hook simply never fires. Re-point it using this document.

## The shape

```ts
interface KildHook {
  agent?: { persona?: string; model?: string; prompt: string };  // spawn one agent, hand it text
  command?: string[];                                            // argv, no shell
}
```

Declaring both runs both. Declaring neither is a no-op. There is no registry, no plugin
lookup, and no hook name the engine recognises — which is the point: kild cannot tell a
memory hook from a backup hook from a notification hook.

## The facts

```
kildId · name · cwd · worktree · base · transcriptPath · ledgerPath
```

- Substituted into `prompt` and into each `command` argv element as `{{fact}}`.
- Passed to `command` as environment: `KILD_CLOSE_KILD_ID`, `KILD_CLOSE_NAME`,
  `KILD_CLOSE_CWD`, `KILD_CLOSE_WORKTREE`, `KILD_CLOSE_BASE`,
  `KILD_CLOSE_TRANSCRIPT_PATH`, `KILD_CLOSE_LEDGER_PATH`.
- Placeholders are `{{…}}`, deliberately **not** `${…}`, so shell expansion stays the hook
  author's. An unknown placeholder is left verbatim rather than blanked.

The same facts go out on the event stream as `{ kildClosed: … }` whether a hook is declared
or not, so a subscribed client can react to the same moment.

The hook is **not awaited** — a slow hook cannot hold a stop open — and every failure (bad
argv, non-zero exit, spawn throw) is logged and swallowed. A hook must never be able to
prevent a kild from stopping.

## Migrating `memory.synthesis`

Before:

```json
{ "memory": { "synthesis": { "model": "…", "persona": "…" } } }
```

After — the same behaviour, with the charter now supplied by you rather than hardcoded in
the engine:

```json
{
  "hooks": {
    "onClose": {
      "agent": {
        "model": "…",
        "persona": "…",
        "prompt": "<charter — see below>"
      }
    }
  }
}
```

`memory.dir` is unchanged and still resolves the ledger directory. Only the synthesis half
moved.

## The charter kild used to ship

This is the exact prompt the engine hardcoded, with the paths replaced by hook placeholders.
It is reproduced here so migrating is copy-paste rather than archaeology — but it is
**intelligence**, so it belongs in PRP or your config, not in the engine.

Note it references `needs-decision[...]`, a protocol that no longer exists in the engine.
Keep that line only if PRP still uses the convention.

```text
[kild memory synthesis] Kild '{{name}}' just stopped in this project.

Inputs:
- Kild transcript (JSON): {{transcriptPath}}
- Engine-written kild log: {{ledgerPath}} — this kild's factual entry is already
  appended; do not duplicate its facts.
- Current curated memory: MEMORY.md in the same directory (may not exist yet)
- Product direction (human-owned, READ-ONLY): direction.md there (may not exist)

Task: read the transcript, then update MEMORY.md so it stays a LEAN curated memory of
this project: key decisions and who made them, important human calls, durable learnings,
and current direction. Compress and rewrite — do not append-and-grow; keep it under ~120
lines of markdown prose (no schemas, no tables of raw facts the log already holds).
Do not modify any other file, do not touch code, do not commit.
```

## Why the engine stopped shipping this

The charter is a description of *how to think about a project's history* — what counts as a
durable learning, how lean is lean, what to compress. That is intelligence, and kild ships
mechanism. The engine's remaining share is the honest half: fire on close, hand over the
facts, run what was declared, and never look inside.

The obligation this creates is recorded in `docs/DEMOLITION.md`: moving intelligence out
means leaving a mechanism counterpart behind. The hook is that counterpart.
