import { reviewCommits } from './git-review.ts';
import { changedFiles, forceRemoveWorktree, registeredWorktree } from './worktree.ts';
import type { BaseSource } from './worktree-status.ts';

/**
 * Disposal — the verb that reclaims a kild's worktree, and the one guard it answers to.
 *
 * **The guard is authored-vs-provisioned, not clean-vs-dirty.** A tree is refused when its
 * branch carries commits base does not have: that is authored work, and refusing is how the
 * operator finds out it was never landed. Uncommitted working-tree state is NOT evidence of
 * anything — provisioning writes files into every tree before an agent runs, and treating
 * that litter as work is exactly what produced 116 permanently unreclaimable trees (27 of
 * 27 sibling trees refused as "dirty" on arrival). So disposal discards the working tree,
 * and it NAMES what it discarded so the call stays auditable.
 *
 * The branch always survives: removing a worktree frees disk, it does not delete work.
 * Every commit on `kild/<name>` is still reachable from the branch afterwards, which is why
 * `force` (deliberately overriding the authored-commits refusal) loses nothing but the
 * working tree.
 *
 * Nothing here is silent: every path returns a reason, because a disposal that quietly
 * declines is the actual defect (see `docs/worktree-disposal.md`).
 */

export type DisposalCode = 'not_found' | 'in_use' | 'authored' | 'undetermined';

export interface DisposalRefusal {
  ok: false;
  code: DisposalCode;
  message: string;
  /** For `authored`: how many commits the branch carries that base does not have. */
  commits?: number;
  /** For `authored`: short sha of the newest of them. */
  tip?: string;
  /** For `authored`: the branch the count was measured against, and whether anybody chose it.
   *  On the wire so a client can render "measured against a guess" rather than a bare number. */
  base?: string;
  baseSource?: BaseSource | 'explicit';
}

/** A disposal the guard allows, and the cost of going ahead with it. */
export interface DisposalPlan {
  ok: true;
  branch?: string;
  /** Uncommitted + untracked files the removal will discard. Named, never hidden.
   *  **Empty means empty only when {@link discardedError} is absent** — see it. */
  discarded: string[];
  /** Why the discard list could not be determined, when git failed to produce one.
   *
   *  This list is the whole of what the operator is told they are about to destroy, and it
   *  used to be `.catch(() => [])`: any git failure rendered as a confident "nothing will
   *  be lost" at the exact moment force-removal was about to lose it. An empty list and an
   *  unanswerable question are not the same claim, and only the caller can decide what to
   *  do about the difference — so it is reported rather than flattened. */
  discardedError?: string;
  /** Authored commits found (non-zero only when `force` overrode the refusal). */
  commits: number;
  tip?: string;
  /** True when the guard would have refused and `force` overrode it. */
  forced: boolean;
}

export type DisposalAssessment = DisposalPlan | DisposalRefusal;

export interface DisposalRequest {
  /** The main checkout the worktree belongs to. */
  repo: string;
  /** The worktree directory to dispose of. */
  dir: string;
  /** Branch the tree is on (`kild/<name>`), for the message. */
  branch?: string;
  /** Base branch authored commits are measured against. Absent → the repo's default. */
  base?: string;
  /** True when a live agent process is working in this tree. */
  inUse: boolean;
  /** Dispose even when the branch carries authored commits. The branch — and every commit
   *  on it — survives regardless; only the working tree is discarded. */
  force?: boolean;
}

/**
 * Apply the guard without touching anything. Returns either a plan (with the files removal
 * would discard) or the refusal, with the reason.
 */
export async function assessDisposal(req: DisposalRequest): Promise<DisposalAssessment> {
  const where = req.branch ?? req.dir;
  if (req.inUse) {
    return {
      ok: false,
      code: 'in_use',
      message: `${where} is in use by a live agent — stop it first`,
    };
  }
  if (!(await registeredWorktree(req.repo, req.dir))) {
    return { ok: false, code: 'not_found', message: `no worktree registered at ${req.dir}` };
  }

  const review = await reviewCommits(req.dir, req.base);
  if (review.error) {
    // Cannot tell whether there is authored work. Refusing is the safe side, and `force`
    // is the way through — but the operator is told which it is, not left with silence.
    if (!req.force) {
      return {
        ok: false,
        code: 'undetermined',
        message:
          `cannot tell whether ${where} carries unlanded work (git: ${review.error}) — ` +
          'retry with force to remove the tree anyway (the branch survives)',
      };
    }
  }
  const commits = review.commits.length;
  const tip = review.commits[0]?.sha.slice(0, 7);
  if (commits > 0 && !req.force) {
    // Say where the base came from when nobody chose it. "carries 365 commits not in main" is
    // a true sentence and a useless one if `main` was a guess — that exact refusal held 116
    // trees and 5.4 GB, because a repo whose default was `dev` read as hundreds of commits
    // ahead of a branch it never forked from. The count is not evidence of unlanded work
    // unless the base is, and only the engine knows which it had.
    const guessed =
      review.baseSource === 'origin-head'
        ? ` (base ${review.base} came from origin/HEAD, which git caches at clone time and ` +
          'never refreshes — set baseBranch in .kild/config.json if it is wrong)'
        : review.baseSource === 'fallback'
          ? ` (nothing configured a base, so this is a guess — set baseBranch in ` +
            '.kild/config.json, or pass --base)'
          : '';
    return {
      ok: false,
      code: 'authored',
      message:
        `${where} carries ${commits} commit${commits === 1 ? '' : 's'} not in ` +
        `${review.base}${tip ? ` (tip ${tip})` : ''}${guessed} — land it, or retry with force ` +
        '(the branch and its commits survive either way)',
      commits,
      tip,
      base: review.base,
      baseSource: review.baseSource,
    };
  }

  // `reviewCommits` above refuses rather than guessing when git cannot answer; this list
  // gets the same treatment one level down. It cannot refuse the disposal — the working
  // tree is not evidence, which is the guard's whole point — but it must not report an
  // empty list it did not observe.
  const discarded = await changedFiles(req.dir).then(
    (files) => ({ files, error: undefined as string | undefined }),
    (err) => ({ files: [], error: err instanceof Error ? err.message : String(err) }),
  );

  return {
    ok: true,
    branch: req.branch,
    discarded: discarded.files,
    discardedError: discarded.error,
    commits,
    tip,
    forced: Boolean(req.force) && commits > 0,
  };
}

/** Remove the worktree. Force by construction: the guard above already answered the only
 *  question that matters, and the working tree is not evidence. The branch is never
 *  deleted — `git worktree remove` does not touch refs, and nothing here adds a
 *  `branch -d`. */
export async function removeKildTree(repo: string, dir: string): Promise<void> {
  const removed = await forceRemoveWorktree(repo, dir);
  if (!removed.ok) throw new Error(`worktree removal failed at ${dir}: ${removed.code}`);
}
