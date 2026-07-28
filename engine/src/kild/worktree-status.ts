import { execFile as execFileCb } from 'node:child_process';
import { promisify } from 'node:util';

// execFile (no shell) mirrors worktree.ts: `dir`/`base` may originate from an
// LLM-driven kild selector, so shell interpolation would be RCE. This module
// is pure observability — a driving agent reads each kild's git state through
// it — so every git failure is captured in `error`, NEVER thrown: a status probe must
// not be able to crash its caller.
const execFile = promisify(execFileCb);

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

/** The git state of one kild directory, relative to a base branch. Every field
 *  has a safe default so a probe failure still yields a well-formed object (see
 *  {@link kildGitStatus}); the failure detail lands in `error`. */
export interface KildGitStatus {
  path: string; // the dir inspected
  branch: string | null;
  base: string; // base branch compared against (default: main)
  /** Where {@link base} came from. `explicit` is an assertion by the caller; the others are
   *  the engine's own resolution, and a client that renders ahead/behind should say so — an
   *  ahead-count measured against a guessed base is not a fact about anybody's work. */
  baseSource: BaseSource;
  ahead: number; // commits on branch not in base
  behind: number; // commits on base not in branch
  dirty: boolean; // uncommitted changes present
  uncommittedFiles: number;
  changedFiles: string[]; // files changed vs base (committed): git diff --name-only <base>...HEAD
  conflictsWithBase: boolean | null; // would HEAD merge into base cleanly? null = undetermined
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

/**
 * Where a base branch came from. Carried alongside the branch itself because a GUESS and a
 * CONFIGURED FACT are different claims, and everything downstream — a git summary, a disposal
 * refusal — reads as authoritative unless told otherwise.
 *
 * This is not hypothetical. 116 worktrees sat unreclaimable behind a refusal reading "carries
 * 365 commits not in main", which was true and meaningless: that repo's default is `dev`, so
 * every tree was hundreds of commits ahead of a branch it had never forked from, by
 * construction. The refusal named the base. What it could not say was that nobody had chosen
 * it. Four measurement passes went into rediscovering that, and 5.4 GB went into not knowing.
 */
export type BaseSource =
  /** A caller named it — `--base`, or a query param. The only one that is an assertion. */
  | 'explicit'
  /** `baseBranch` in project or global config. Chosen deliberately, just not in this call. */
  | 'configured'
  /** The checkout's current branch at creation time. Plausible, nobody chose it. */
  | 'current-branch'
  /** `origin/HEAD` — the remote's default AS CACHED LOCALLY. Git writes this symref at clone
   *  time and never refreshes it, so it goes stale silently when the remote's default moves.
   *  Better than a literal guess; still not a fact anyone asserted. */
  | 'origin-head'
  /** Nothing said otherwise, so `main`. A pure guess, and the one that cost the 116. */
  | 'fallback'
  /** A kild recorded before provenance was tracked. Not a guess and not a choice — an
   *  absence. Named rather than folded into `fallback`, because claiming we know it was
   *  guessed is the same overreach as claiming we know it was chosen. */
  | 'unrecorded';

export interface ResolvedBase {
  base: string;
  source: BaseSource;
}

/** The base branch to compare against when the caller doesn't name one: the remote's
 *  default (`origin/HEAD`, minus the `origin/` prefix) if set, else `main` — and WHICH of
 *  those it was. Shared with git-review so summary status and review drill-down agree. */
export async function resolveDefaultBase(dir: string): Promise<ResolvedBase> {
  const head = await runGit(dir, ['symbolic-ref', '--short', 'refs/remotes/origin/HEAD']);
  if (head.ok) {
    const branch = head.stdout.trim().replace(/^origin\//, '');
    if (branch) return { base: branch, source: 'origin-head' };
  }
  return { base: 'main', source: 'fallback' };
}

/** Inspect one kild directory's git state relative to `base` (default: the
 *  remote default branch, else `main`). Never throws: a non-git dir, a missing base
 *  ref, or any git error returns a well-formed object with `error` set and safe
 *  defaults so a driving agent can surface the state without crashing. */
export async function kildGitStatus(dir: string, base?: ResolvedBase): Promise<KildGitStatus> {
  // Takes a ResolvedBase, never a bare string. A string parameter meant any caller holding a
  // stored base passed it as though a human had named it, so `Kild.base` — itself resolved by
  // a fallback chain at creation — was reported as `explicit` for every live kild. The base and
  // where it came from travel together or the provenance is decoration.
  const resolved = base ?? (await resolveDefaultBase(dir));
  const resolvedBase = resolved.base;
  const status: KildGitStatus = {
    path: dir,
    branch: null,
    base: resolvedBase,
    baseSource: resolved.source,
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

  // Would merging HEAD into base conflict? `merge-tree --write-tree` exits 0 (clean) /
  // 1 (conflicts) / 128 (error); a git too old to support --write-tree also fails. The
  // exit code rides err.code on the rejection, so this call bypasses runGit (which drops
  // it). Anything other than a clean 0 or a conflicting 1 stays null (undetermined) —
  // never thrown. Nothing to merge when the branch isn't ahead → no conflict.
  if (status.ahead === 0) {
    status.conflictsWithBase = false;
  } else {
    try {
      await execFile('git', ['-C', dir, 'merge-tree', '--write-tree', resolvedBase, 'HEAD']);
      status.conflictsWithBase = false;
    } catch (err) {
      const code = (err as { code?: unknown }).code;
      status.conflictsWithBase = code === 1 ? true : null;
    }
  }

  return status;
}
