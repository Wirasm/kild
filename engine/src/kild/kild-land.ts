import { execFile as execFileCb } from 'node:child_process';
import { promisify } from 'node:util';

import { type ReviewCommit, reviewCommits } from './git-review.ts';
import { currentBranch } from './worktree.ts';
import type { ResolvedBase } from './worktree-status.ts';
import { kildGitStatus } from './worktree-status.ts';

/**
 * Landing — merging a kild's branch into its base.
 *
 * Two calls over one shape: {@link landPlan} answers "would this merge, and what would it
 * carry" and **touches nothing** (read-only git plus `merge-tree`, which writes objects to
 * the odb but no ref, index or working tree); {@link landMerge} performs the merge and
 * reports the sha it became. Same result shape either way, so a dry run and a real land are
 * comparable line for line.
 *
 * The git facts come from the existing probes (`git-review`, `worktree-status`) — nothing
 * here re-derives commits, files or conflicts.
 *
 * The merge happens in the project's MAIN CHECKOUT, on the base branch itself, which is
 * where a human would do it. That means the base must actually be checked out there and the
 * checkout must be clean; anything else is reported as an error rather than worked around,
 * because silently landing somewhere unexpected is worse than not landing.
 */
const execFile = promisify(execFileCb);

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

export interface LandResult {
  /** Base branch the kild would land into. */
  base: string;
  /** Branch being landed, when git could say. */
  branch: string | null;
  /** Commits the land carries over (newest first) — empty means nothing to land. */
  commits: ReviewCommit[];
  /** Files the branch changed vs base (committed). */
  files: string[];
  /** Paths that conflict when merging into base — the collision preview. */
  collides: string[];
  /** True when the merge would apply (or did apply) cleanly. */
  wouldMerge: boolean;
  /** True only for a merge that actually happened. */
  merged: boolean;
  /** The merge commit — present only when `merged`. */
  sha?: string;
  /** Why it would not / did not land, or any git failure. Never thrown. */
  error?: string;
}

type GitResult = { ok: true; stdout: string } | { ok: false; error: string };

async function runGit(dir: string, args: string[]): Promise<GitResult> {
  try {
    const { stdout } = await execFile('git', ['-C', dir, ...args]);
    return { ok: true, stdout };
  } catch (err) {
    return { ok: false, error: errText(err) };
  }
}

/** Conflicting paths for merging `branch` into `base`, via `merge-tree --write-tree`:
 *  exit 0 = clean, exit 1 = conflicts and stdout is `<tree-oid>\n<path>…\n\n<messages>`.
 *  Anything else (an old git, a bad ref) is undetermined — reported, never guessed at.
 *  Bypasses `runGit`: the conflict list arrives on the REJECTION's stdout. */
export async function conflictingPaths(
  dir: string,
  base: string,
  branch: string,
): Promise<{ collides: string[]; error?: string }> {
  try {
    await execFile('git', ['-C', dir, 'merge-tree', '--write-tree', '--name-only', base, branch]);
    return { collides: [] };
  } catch (err) {
    const failure = err as { code?: unknown; stdout?: unknown };
    if (failure.code !== 1 || typeof failure.stdout !== 'string') {
      return { collides: [], error: errText(err) };
    }
    const collides: string[] = [];
    for (const line of failure.stdout.split('\n').slice(1)) {
      if (line === '') break;
      collides.push(line);
    }
    return { collides };
  }
}

/**
 * The dry run: what landing this kild would do. Touches no ref, no index and no working
 * tree — it is safe to call on every render.
 *
 * `dir` is the kild's effective directory (its worktree, else its cwd); `base` its base
 * branch (absent → the repo's default).
 */
export async function landPlan(dir: string, base?: ResolvedBase): Promise<LandResult> {
  const status = await kildGitStatus(dir, base);
  const result: LandResult = {
    base: status.base,
    branch: status.branch,
    commits: [],
    files: status.changedFiles,
    collides: [],
    wouldMerge: false,
    merged: false,
  };
  if (status.error) {
    result.error = status.error;
    return result;
  }
  if (status.branch === null) {
    result.error = 'the kild directory is not on a branch (detached HEAD) — nothing to land';
    return result;
  }
  if (status.branch === status.base) {
    result.error = `the kild ran on ${status.base} itself — there is no branch to land`;
    return result;
  }

  const review = await reviewCommits(dir, { base: status.base, source: status.baseSource });
  if (review.error) {
    result.error = review.error;
    return result;
  }
  result.commits = review.commits;
  if (review.commits.length === 0) {
    result.error = `nothing committed on ${status.branch} vs ${status.base}`;
    return result;
  }

  const conflicts = await conflictingPaths(dir, status.base, status.branch);
  result.collides = conflicts.collides;
  if (conflicts.error) {
    result.error = conflicts.error;
    return result;
  }
  if (conflicts.collides.length > 0) {
    result.error =
      `merging ${status.branch} into ${status.base} conflicts in ` +
      `${conflicts.collides.length} file${conflicts.collides.length === 1 ? '' : 's'}`;
    return result;
  }
  result.wouldMerge = true;
  return result;
}

/**
 * Execute the land: merge the kild's branch into base in `repo` (the main checkout) and
 * report the merge sha. Runs the same plan first and refuses on anything it flagged, so a
 * caller never has to interpret two different verdicts.
 */
export async function landMerge(
  repo: string,
  dir: string,
  base?: ResolvedBase,
): Promise<LandResult> {
  const plan = await landPlan(dir, base);
  if (!plan.wouldMerge || plan.branch === null) return plan;

  const on = await currentBranch(repo);
  if (on !== plan.base) {
    plan.wouldMerge = false;
    plan.error =
      `${repo} is on ${on ?? 'a detached HEAD'}, not ${plan.base} — ` +
      `check out ${plan.base} there to land into it`;
    return plan;
  }
  const dirty = await runGit(repo, ['status', '--porcelain']);
  if (dirty.ok && dirty.stdout.trim() !== '') {
    plan.wouldMerge = false;
    plan.error = `${repo} has uncommitted changes on ${plan.base} — commit or stash them first`;
    return plan;
  }

  const merge = await runGit(repo, [
    'merge',
    '--no-ff',
    '-m',
    `kild: land ${plan.branch} into ${plan.base}`,
    plan.branch,
  ]);
  if (!merge.ok) {
    plan.wouldMerge = false;
    plan.error = merge.error;
    return plan;
  }
  const head = await runGit(repo, ['rev-parse', 'HEAD']);
  plan.merged = true;
  if (head.ok) plan.sha = head.stdout.trim();
  else plan.error = head.error; // merged, but we cannot name it — say so
  return plan;
}
