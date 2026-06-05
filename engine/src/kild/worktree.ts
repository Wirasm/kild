import { execFile as execFileCb } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { promisify } from 'node:util';

import { kildHome } from './config.ts';

// execFile (no shell) + a branch-name allowlist: the brain's create_worktree tool
// and the cockpit's worktree selector feed a (possibly LLM-generated) name in here,
// so shell interpolation would be RCE. This module is kild-owned and hot-path-safe:
// the session path imports it directly, so it must NOT depend on @flue (the general
// `worktree()` sandbox lives in flue/worktree-sandbox.ts).
const execFile = promisify(execFileCb);

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

/** A git worktree on disk — kild's local isolation strategy. `branch` is the full
 *  `kild/<name>` ref, `path` the deterministic dir under `$KILD_HOME/worktrees`;
 *  `name` is the kild worktree name (branch minus the `kild/` prefix), undefined for
 *  non-kild worktrees. */
export interface Worktree {
  branch: string;
  path: string;
  name?: string;
}

export function worktreesRoot(): string {
  return path.join(kildHome(), 'worktrees');
}

export function assertSafeBranch(branch: string): void {
  if (branch.startsWith('-') || !/^[A-Za-z0-9._/-]+$/.test(branch)) {
    throw new Error(`invalid branch name: ${branch}`);
  }
}

/** The `kild/<name>` branch ref a worktree name maps to. Deterministic, no I/O. */
export function worktreeRef(name: string): string {
  assertSafeBranch(name);
  return `kild/${name}`;
}

/** The kild worktree name a `kild/<name>` branch ref carries (the inverse of
 *  {@link worktreeRef}). Non-kild refs pass through unchanged. */
export function worktreeName(branch: string): string {
  return branch.replace(/^kild\//, '');
}

/** The on-disk path a worktree name maps to. Deterministic, no I/O — so the engine
 *  can fill `SessionInfo.worktreePath` synchronously before the worker creates it. */
export function worktreePath(name: string): string {
  assertSafeBranch(name);
  return path.join(worktreesRoot(), name.replace(/\//g, '-'));
}

/** Create a fresh isolated worktree on a `kild/<branch>` branch, force-resetting any
 *  pre-existing one. For the brain's explicit "new worktree" — NOT the session path
 *  (which must never reset a shared tree; use {@link ensureWorktree}). */
export async function createWorktree(repo: string, branch: string): Promise<Worktree> {
  assertSafeBranch(branch);
  const wtPath = worktreePath(branch);
  const ref = worktreeRef(branch);
  // Best-effort pre-clean of a same-named worktree before the force re-create.
  // Force is intentional here ("new worktree" is destructive-by-request).
  await execFile('git', ['-C', repo, 'worktree', 'remove', '--force', wtPath]).catch(() => {});
  await execFile('git', ['-C', repo, 'worktree', 'add', '-B', ref, wtPath]);
  return { branch: ref, path: wtPath, name: branch };
}

/** Create the worktree if missing, ATTACH (reuse) it if it already exists. The
 *  session path uses this — naming the same worktree as another session must join
 *  its tree, never reset it (which would blow away a coder's work when a reviewer
 *  joins). Never resets: an existing dir attaches; an existing *branch* (worktree
 *  removed but branch kept) is checked out, preserving its commits. */
export async function ensureWorktree(repo: string, name: string): Promise<Worktree> {
  const wtPath = worktreePath(name);
  const ref = worktreeRef(name);
  const attached = { branch: ref, path: wtPath, name };
  if (existsSync(wtPath)) {
    // Attach only to a real linked worktree (the `.git` pointer file). A leftover or
    // corrupt dir must NOT silently become a non-isolated cwd — fail fast instead.
    if (existsSync(path.join(wtPath, '.git'))) return attached;
    throw new Error(`worktree path exists but is not a git worktree: ${wtPath}`);
  }
  try {
    // The branch may already exist (the worktree was removed but the branch kept).
    // Check it out — never `-B` (which would reset and lose its commits).
    const branchExists = await execFile('git', ['-C', repo, 'rev-parse', '--verify', ref])
      .then(() => true)
      .catch(() => false);
    if (branchExists) {
      await execFile('git', ['-C', repo, 'worktree', 'add', wtPath, ref]);
    } else {
      await execFile('git', ['-C', repo, 'worktree', 'add', '-b', ref, wtPath]);
    }
  } catch (err) {
    // Cold-start race: a concurrent session creating the *same* new worktree between
    // our existsSync check and `worktree add` wins, and ours fails ("already exists").
    // N agents sharing one fresh tree is valid, so attach to the real worktree it left
    // behind; only re-throw if the path still isn't a git worktree.
    if (existsSync(path.join(wtPath, '.git'))) return attached;
    throw err;
  }
  return attached;
}

export async function listWorktrees(repo: string): Promise<Worktree[]> {
  const { stdout } = await execFile('git', ['-C', repo, 'worktree', 'list', '--porcelain']);
  const trees: Worktree[] = [];
  let cur: Partial<Worktree> = {};
  const push = () => {
    if (!cur.path) return;
    const branch = cur.branch ?? '(detached)';
    trees.push({
      path: cur.path,
      branch,
      name: branch.startsWith('kild/') ? worktreeName(branch) : undefined,
    });
    cur = {};
  };
  for (const line of stdout.split('\n')) {
    if (line.startsWith('worktree ')) {
      push(); // flush the previous record (porcelain separates with a blank line)
      cur = { path: line.slice('worktree '.length) };
    } else if (line.startsWith('branch '))
      cur.branch = line.slice('branch '.length).replace('refs/heads/', '');
  }
  push();
  return trees;
}

export async function removeWorktree(repo: string, wtPath: string): Promise<void> {
  await execFile('git', ['-C', repo, 'worktree', 'remove', '--force', wtPath]);
}

/** The repo's default branch: `origin/HEAD` if set, else `main`/`master` if they
 *  exist, else the current branch. Used to decide which `kild/*` branches are merged. */
async function defaultBranch(repo: string): Promise<string> {
  const head = await execFile('git', ['-C', repo, 'rev-parse', '--abbrev-ref', 'origin/HEAD'])
    .then(({ stdout }) => stdout.trim().replace(/^origin\//, ''))
    .catch(() => '');
  if (head) return head;
  for (const candidate of ['main', 'master']) {
    const ok = await execFile('git', ['-C', repo, 'rev-parse', '--verify', candidate])
      .then(() => true)
      .catch(() => false);
    if (ok) return candidate;
  }
  const { stdout } = await execFile('git', ['-C', repo, 'rev-parse', '--abbrev-ref', 'HEAD']);
  return stdout.trim();
}

// Coalesce concurrent prunes per repo: prune runs on engine start, on every worktree
// list, and from the CLI — concurrent git invocations would race the index lock. The
// keep set is identical across any near-simultaneous engine callers, so sharing the
// in-flight result is safe.
const pruneInFlight = new Map<string, Promise<string[]>>();

/** The one automatic cleanup: for each `kild/*` worktree whose branch is fully merged
 *  into the repo's default branch, remove the worktree and `-d`-delete the branch.
 *  Returns the worktree names pruned. `keep` names worktrees a live session is using —
 *  those are never pruned.
 *
 *  Data-safety: the remove is **non-force**. git refuses to remove a worktree with
 *  uncommitted/untracked changes, so a merged branch that still has new uncommitted
 *  edits is preserved (only clean, fully-integrated trees are removed). `branch -d`
 *  (safe) refuses unmerged as a backstop. */
export function pruneMergedWorktrees(
  repo: string,
  keep: Set<string> = new Set(),
): Promise<string[]> {
  const existing = pruneInFlight.get(repo);
  if (existing) return existing;
  const p = doPruneMerged(repo, keep).finally(() => pruneInFlight.delete(repo));
  pruneInFlight.set(repo, p);
  return p;
}

async function doPruneMerged(repo: string, keep: Set<string>): Promise<string[]> {
  const base = await defaultBranch(repo);
  const { stdout } = await execFile('git', ['-C', repo, 'branch', '--merged', base]);
  const merged = new Set(
    stdout
      .split('\n')
      .map((l) => l.replace(/^[*+]?\s*/, '').trim())
      .filter(Boolean),
  );
  const pruned: string[] = [];
  for (const wt of await listWorktrees(repo)) {
    const ref = wt.branch; // e.g. "kild/fix-auth"
    if (!ref.startsWith('kild/')) continue;
    if (ref === `kild/${base}`) continue; // never prune the default branch itself
    if (!merged.has(ref)) continue;
    const name = worktreeName(ref);
    if (keep.has(name)) continue;
    try {
      await execFile('git', ['-C', repo, 'worktree', 'remove', wt.path]); // non-force: preserves dirty trees
    } catch {
      continue; // dirty or in use → leave the worktree (and its branch) intact
    }
    try {
      await execFile('git', ['-C', repo, 'branch', '-d', ref]);
    } catch (err) {
      // Worktree gone (disk freed) but the branch lingered — log, don't hide it.
      console.warn(`kild: removed worktree ${name} but could not delete ${ref}: ${errText(err)}`);
    }
    pruned.push(name);
  }
  return pruned;
}
