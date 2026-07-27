# Worktree disposal — why trees accumulate

A measured, reproducible failure: **116 unreclaimable worktrees** on one machine, with 27 real
agent trees found dirty *on arrival* — before any agent had touched them.

Not a demolition item. A live bug in code the demolition explicitly keeps, found while auditing.

## Root cause

Three behaviours compose into unbounded accumulation.

**1. Any untracked file makes a tree "dirty."** `changedFiles()` in `worktree.ts` runs:

```
git status --porcelain --untracked-files=all
```

`--untracked-files=all` means a single stray file counts exactly the same as an uncommitted
change to tracked work.

**2. Removal refuses on dirty.** `removeWorktree()` returns `{ok: false, code: 'dirty'}` the
moment `files.length > 0`.

**3. Prune gives up silently.** `pruneMergedWorktrees()` attempts a non-force remove and
swallows the failure:

```js
} catch {
  continue; // dirty or in use → leave the worktree (and its branch) intact
}
```

So: tooling writes residue (`.archon` YAML) into every tree at creation → every tree is dirty
before an agent starts → prune refuses → the `catch` hides it → trees accumulate forever, and
nothing anywhere reports that reclamation is failing.

Each behaviour is individually defensible. Together they are a leak with no indicator.

## The two real defects

**Silence.** The `catch { continue }` is the one that turned a policy disagreement into 116
trees. A prune that cannot prune must say so. Had it reported skips from the start, this would
have surfaced at tree three, not tree 116.

**Conflating residue with work.** An uncommitted change to a tracked file is *work at risk* —
refusing to delete it is correct. An untracked file in a tree whose branch has **already
merged** is almost always tooling residue. Same refusal, very different stakes.

## Fix direction

1. **Report, never swallow.** `pruneMergedWorktrees` returns what it skipped and why
   (`{pruned: [], skipped: [{name, reason, files}]}`). The CLI and UI surface it. This alone
   makes the leak self-announcing and should land first — it is small and purely additive.
2. **Separate tracked-modified from untracked-only.** `changedFiles()` should report the two
   classes distinctly rather than flattening both into "dirty".
3. **Merged + untracked-only is reclaimable.** If the branch already landed in the base, the
   tracked work is preserved by definition; only residue would be lost. Reclaim it, and name
   the discarded files in the result so it stays auditable. Tracked modifications keep refusing.

## The fix that is not in kild

The cheapest correct fix is upstream of the engine: **the tooling writing `.archon` into every
worktree should gitignore it.** `git status --porcelain` already respects `.gitignore`, so
ignored residue would stop registering as dirty and all three behaviours above would work as
designed.

That does not excuse the silent `catch` — kild must still report what it could not reclaim. But
it does mean the accumulation is fixable today, from the writer's side, without waiting on any
engine change.

## Reclaiming what is already stranded

`forceRemoveWorktree` (`kild worktree rm --force`) still works and bypasses the dirty check —
it is the explicit destructive verb, which is exactly what this situation calls for. Confirm the
branch is merged before forcing, because force discards untracked files without listing them.
