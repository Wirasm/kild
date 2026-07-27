import { realpathSync } from 'node:fs';

import { listWorktrees } from './worktree.ts';

/**
 * The kild inventory **as git knows it**.
 *
 * A kild is a worktree, so the authoritative list of kilds is not the engine's in-memory
 * registry — it is `git worktree list`. The registry only knows the kilds THIS engine
 * process created; every `kild/*` tree left by an earlier run is invisible to it, and an
 * invisible tree has no id, so nothing could address it, list it or dispose of it. That is
 * how trees became permanently unreclaimable.
 *
 * This module answers only "which `kild/*` worktrees exist, and where". Whether one has a
 * live kild behind it is the caller's join (see {@link orphanTrees}).
 */

/** One `kild/*` worktree on disk. */
export interface KildTree {
  /** Worktree name — the branch minus the `kild/` prefix. This is the address of a tree
   *  with no live kild record. */
  worktree: string;
  /** Full branch ref, e.g. `kild/fix-auth`. */
  branch: string;
  /** The worktree directory. */
  path: string;
  /** The main checkout this worktree belongs to. */
  repo: string;
}

function samePath(a: string, b: string): boolean {
  try {
    return realpathSync(a) === realpathSync(b);
  } catch {
    return a === b;
  }
}

/**
 * Every `kild/*` worktree across `repos`, deduplicated by path.
 *
 * A worktree that is **not** a kild is skipped outright: only a `kild/<name>` branch is a
 * kild, so the repo's own checkout, a detached tree and any hand-made worktree on another
 * branch never appear here — kild does not claim, list or dispose of trees it did not
 * create. A repo git cannot read (an unregistered path, a deleted project dir) contributes
 * nothing rather than failing the whole inventory.
 */
export async function kildTrees(repos: Iterable<string>): Promise<KildTree[]> {
  const found = new Map<string, KildTree>();
  for (const repo of new Set(repos)) {
    let trees: Awaited<ReturnType<typeof listWorktrees>>;
    try {
      trees = await listWorktrees(repo);
    } catch {
      continue; // not a git repo / gone — it holds no kild trees for us
    }
    for (const tree of trees) {
      if (!tree.name || !tree.branch.startsWith('kild/')) continue;
      // The main checkout itself, even if the user happens to have a kild/* branch out
      // there: it is their working copy, not a disposable kild tree.
      if (samePath(tree.path, repo)) continue;
      if (found.has(tree.path)) continue;
      found.set(tree.path, {
        worktree: tree.name,
        branch: tree.branch,
        path: tree.path,
        repo,
      });
    }
  }
  return [...found.values()];
}

/** The trees no live kild is using — kilds whose record is gone. They are addressed by
 *  worktree name, which is the only id they have. */
export function orphanTrees(trees: KildTree[], liveWorktrees: Set<string>): KildTree[] {
  return trees.filter((tree) => !liveWorktrees.has(tree.worktree));
}
