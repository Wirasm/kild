import { execFile as execFileCb } from 'node:child_process';
import { promisify } from 'node:util';

// execFile (no shell) mirrors worktree.ts: `dir`/`base` may originate from an
// LLM-driven workstream selector, so shell interpolation would be RCE. This module
// is pure observability — a driving agent reads each workstream's git state through
// it — so every git failure is captured in `error`, NEVER thrown: a status probe must
// not be able to crash its caller.
const execFile = promisify(execFileCb);

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

/** The git state of one workstream directory, relative to a base branch. Every field
 *  has a safe default so a probe failure still yields a well-formed object (see
 *  {@link workstreamGitStatus}); the failure detail lands in `error`. */
export interface WorkstreamGitStatus {
  path: string; // the dir inspected
  branch: string | null;
  base: string; // base branch compared against (default: main)
  ahead: number; // commits on branch not in base
  behind: number; // commits on base not in branch
  dirty: boolean; // uncommitted changes present
  uncommittedFiles: number;
  changedFiles: string[]; // files changed vs base (committed): git diff --name-only <base>...HEAD
  conflictsWithBase: boolean | null; // leave null for now (a later slice fills it)
  error?: string; // any git failure captured here, NEVER thrown
}

type GitResult = { ok: true; stdout: string } | { ok: false; error: string };

/** Run a git command under `dir`, capturing failure as data instead of throwing. */
async function runGit(dir: string, args: string[]): Promise<GitResult> {
  try {
    const { stdout } = await execFile('git', ['-C', dir, ...args]);
    return { ok: true, stdout };
  } catch (err) {
    return { ok: false, error: errText(err) };
  }
}

/** The base branch to compare against when the caller doesn't name one: the remote's
 *  default (`origin/HEAD`, minus the `origin/` prefix) if set, else `main`. */
async function resolveDefaultBase(dir: string): Promise<string> {
  const head = await runGit(dir, ['symbolic-ref', '--short', 'refs/remotes/origin/HEAD']);
  if (head.ok) {
    const branch = head.stdout.trim().replace(/^origin\//, '');
    if (branch) return branch;
  }
  return 'main';
}

/** Inspect one workstream directory's git state relative to `base` (default: the
 *  remote default branch, else `main`). Never throws: a non-git dir, a missing base
 *  ref, or any git error returns a well-formed object with `error` set and safe
 *  defaults so a driving agent can surface the state without crashing. */
export async function workstreamGitStatus(
  dir: string,
  base?: string,
): Promise<WorkstreamGitStatus> {
  const resolvedBase = base ?? (await resolveDefaultBase(dir));
  const status: WorkstreamGitStatus = {
    path: dir,
    branch: null,
    base: resolvedBase,
    ahead: 0,
    behind: 0,
    dirty: false,
    uncommittedFiles: 0,
    changedFiles: [],
    conflictsWithBase: null,
  };

  // Current branch. Failure here means not a git repo (or a broken one) — bail with
  // safe defaults; the remaining probes would only repeat the same failure.
  const branch = await runGit(dir, ['rev-parse', '--abbrev-ref', 'HEAD']);
  if (!branch.ok) {
    status.error = branch.error;
    return status;
  }
  status.branch = branch.stdout.trim() || null;

  // Working-tree cleanliness is base-independent, so report it even when the base
  // ref is missing below. Any porcelain line means dirty; the line count is the file count.
  const porcelain = await runGit(dir, ['status', '--porcelain']);
  if (porcelain.ok) {
    const lines = porcelain.stdout.split('\n').filter((line) => line.length > 0);
    status.uncommittedFiles = lines.length;
    status.dirty = lines.length > 0;
  } else {
    status.error = porcelain.error;
  }

  // A base ref the repo doesn't have (unknown branch, or a fresh repo without it) is a
  // valid state, not a crash: keep ahead/behind at 0 and changedFiles empty, note it.
  const baseExists = await runGit(dir, [
    'rev-parse',
    '--verify',
    '--quiet',
    `${resolvedBase}^{commit}`,
  ]);
  if (!baseExists.ok) {
    status.error = `base ref not found: ${resolvedBase}`;
    return status;
  }

  // ahead/behind: `--left-right --count base...HEAD` prints "<left>\t<right>" where
  // left = commits in base not in HEAD (behind), right = commits in HEAD not in base (ahead).
  const counts = await runGit(dir, [
    'rev-list',
    '--left-right',
    '--count',
    `${resolvedBase}...HEAD`,
  ]);
  if (counts.ok) {
    const [behind, ahead] = counts.stdout.trim().split(/\s+/);
    status.behind = Number.parseInt(behind ?? '', 10) || 0;
    status.ahead = Number.parseInt(ahead ?? '', 10) || 0;
  } else {
    status.error = counts.error;
  }

  // Files changed on the branch vs base (committed only — uncommitted edits are the
  // dirty/uncommittedFiles fields above).
  const diff = await runGit(dir, ['diff', '--name-only', `${resolvedBase}...HEAD`]);
  if (diff.ok) {
    status.changedFiles = diff.stdout.split('\n').filter((file) => file.length > 0);
  } else {
    status.error = diff.error;
  }

  return status;
}
