import { execFile as execFileCb } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';

import { resolveDefaultBase } from './worktree-status.ts';

/**
 * Review intelligence — the git drill-down behind a review surface. Where
 * worktree-status answers "how far along is this workstream?" (summary), this module
 * answers "what exactly changed?": the commits vs base, per-file diff stats
 * (committed + uncommitted), and one file's unified patch.
 *
 * Same contract as worktree-status: execFile (no shell — `dir`/`base` may originate
 * from an LLM-driven caller, so shell interpolation would be RCE), and every git
 * failure is captured in `error`, NEVER thrown — a review probe must not be able to
 * crash its caller.
 */
const execFile = promisify(execFileCb);

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

/** Diff endpoint payload cap (~200 KB of patch text) — a review surface wants the
 *  patch, not a memory bomb; past this the result is truncated and flagged. */
export const DIFF_CAP = 200 * 1024;

/** Headroom for raw git output before our own cap applies (execFile default is 1 MB,
 *  which a real diff easily exceeds). */
const MAX_BUFFER = 64 * 1024 * 1024;

/** One commit on the workstream branch that base doesn't have. `ts` is epoch millis
 *  (matching `RoomMessage.ts`). */
export interface ReviewCommit {
  sha: string;
  subject: string;
  author: string;
  ts: number;
  filesChanged: number;
  additions: number;
  deletions: number;
}

export type ReviewFileStatus = 'added' | 'modified' | 'deleted' | 'renamed';

/** Per-file diff stats vs base — committed and working-tree changes combined.
 *  `uncommitted` marks files with working-tree/index changes not yet committed. */
export interface ReviewFile {
  path: string;
  additions: number;
  deletions: number;
  status: ReviewFileStatus;
  uncommitted: boolean;
  /** For `renamed`: the pre-rename path. */
  renamedFrom?: string;
}

export interface ReviewCommitsResult {
  base: string;
  commits: ReviewCommit[];
  error?: string; // any git failure captured here, NEVER thrown
}

export interface ReviewFilesResult {
  base: string;
  files: ReviewFile[];
  error?: string; // any git failure captured here, NEVER thrown
}

export interface ReviewDiffResult {
  base: string;
  path: string;
  patch: string;
  truncated: boolean;
  error?: string; // any git failure captured here, NEVER thrown
  /** Set when `path` is not a file git itself reported as changed — the traversal
   *  guard; the HTTP layer maps it to a 404. */
  unknownPath?: boolean;
}

type GitResult = { ok: true; stdout: string } | { ok: false; error: string };

/** Run a git command under `dir`, capturing failure as data instead of throwing. */
async function runGit(dir: string, args: string[]): Promise<GitResult> {
  try {
    const { stdout } = await execFile('git', ['-C', dir, ...args], { maxBuffer: MAX_BUFFER });
    return { ok: true, stdout };
  } catch (err) {
    return { ok: false, error: errText(err) };
  }
}

/** HEAD and the base ref must both resolve before any comparison is meaningful.
 *  Returns the error string (worktree-status wording), or undefined when fine. */
async function verifyRepoAndBase(dir: string, base: string): Promise<string | undefined> {
  const head = await runGit(dir, ['rev-parse', '--verify', '--quiet', 'HEAD^{commit}']);
  if (!head.ok) return head.error; // not a git repo, or no commits yet
  const baseExists = await runGit(dir, ['rev-parse', '--verify', '--quiet', `${base}^{commit}`]);
  if (!baseExists.ok) return `base ref not found: ${base}`;
  return undefined;
}

// ── Pure parsers (exported for unit tests) ────────────────────────────────────

// git log --format uses these separators so subjects with tabs/newlines can't break
// parsing: \x1e starts each commit record, \x1f separates header fields.
const RECORD_SEP = '\x1e';
const FIELD_SEP = '\x1f';
const LOG_FORMAT = `${RECORD_SEP}%H${FIELD_SEP}%s${FIELD_SEP}%an${FIELD_SEP}%at`;

/** Parse `git log --numstat --format=<LOG_FORMAT>` output: one record per commit —
 *  a header line, then numstat lines (`adds\tdels\tpath`; binary files show `-`). */
export function parseCommitLog(stdout: string): ReviewCommit[] {
  const commits: ReviewCommit[] = [];
  for (const record of stdout.split(RECORD_SEP)) {
    if (!record.trim()) continue;
    const lines = record.split('\n');
    const [sha, subject, author, at] = (lines[0] ?? '').split(FIELD_SEP);
    if (!sha) continue;
    const commit: ReviewCommit = {
      sha,
      subject: subject ?? '',
      author: author ?? '',
      ts: (Number.parseInt(at ?? '', 10) || 0) * 1000,
      filesChanged: 0,
      additions: 0,
      deletions: 0,
    };
    for (const line of lines.slice(1)) {
      const stat = /^(\d+|-)\t(\d+|-)\t(.+)$/.exec(line);
      if (!stat) continue;
      commit.filesChanged += 1;
      if (stat[1] !== '-') commit.additions += Number.parseInt(stat[1] as string, 10);
      if (stat[2] !== '-') commit.deletions += Number.parseInt(stat[2] as string, 10);
    }
    commits.push(commit);
  }
  return commits;
}

/** Parse `git diff --numstat -z` output. Plain entry: `adds\tdels\tpath NUL`;
 *  rename entry: `adds\tdels\t NUL oldpath NUL newpath NUL` (keyed by the new path).
 *  Binary files report `-\t-` — counted as 0/0. */
export function parseNumstatZ(stdout: string): Array<{
  path: string;
  additions: number;
  deletions: number;
}> {
  const tokens = stdout.split('\0');
  const entries: Array<{ path: string; additions: number; deletions: number }> = [];
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (!token) continue;
    const stat = /^(\d+|-)\t(\d+|-)\t(.*)$/s.exec(token);
    if (!stat) continue;
    const additions = stat[1] === '-' ? 0 : Number.parseInt(stat[1] as string, 10);
    const deletions = stat[2] === '-' ? 0 : Number.parseInt(stat[2] as string, 10);
    let filePath = stat[3] as string;
    if (filePath === '') {
      // Rename: the next two NUL-separated tokens are the old and new path.
      i += 2;
      filePath = tokens[i] ?? '';
    }
    if (filePath) entries.push({ path: filePath, additions, deletions });
  }
  return entries;
}

/** Parse `git diff --name-status -z` output: `STATUS NUL path NUL`, with renames/
 *  copies as `R<score> NUL oldpath NUL newpath NUL`. Typechange and anything exotic
 *  fold into `modified`. */
export function parseNameStatusZ(stdout: string): Array<{
  path: string;
  status: ReviewFileStatus;
  renamedFrom?: string;
}> {
  const tokens = stdout.split('\0');
  const entries: Array<{ path: string; status: ReviewFileStatus; renamedFrom?: string }> = [];
  for (let i = 0; i < tokens.length; i++) {
    const code = tokens[i];
    if (!code) continue;
    const kind = code[0];
    if (kind === 'R' || kind === 'C') {
      const from = tokens[++i] ?? '';
      const to = tokens[++i] ?? '';
      if (!to) continue;
      if (kind === 'R') entries.push({ path: to, status: 'renamed', renamedFrom: from });
      else entries.push({ path: to, status: 'added' }); // a copy is a new file
      continue;
    }
    const filePath = tokens[++i] ?? '';
    if (!filePath) continue;
    const status: ReviewFileStatus = kind === 'A' ? 'added' : kind === 'D' ? 'deleted' : 'modified';
    entries.push({ path: filePath, status });
  }
  return entries;
}

/** Parse `git status --porcelain -z` output into the set of paths with uncommitted
 *  changes plus the untracked subset. Entry: `XY<space>path NUL`; a staged rename adds
 *  a trailing `origpath NUL` token (skipped — the new path is the identity). */
export function parsePorcelainZ(stdout: string): {
  uncommitted: Set<string>;
  untracked: string[];
} {
  const tokens = stdout.split('\0');
  const uncommitted = new Set<string>();
  const untracked: string[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const entry = tokens[i];
    if (!entry || entry.length < 4 || entry[2] !== ' ') continue;
    const code = entry.slice(0, 2);
    const filePath = entry.slice(3);
    uncommitted.add(filePath);
    if (code === '??') untracked.push(filePath);
    if (code[0] === 'R' || code[0] === 'C') i += 1; // skip the orig-path token
  }
  return { uncommitted, untracked };
}

// ── Probes ────────────────────────────────────────────────────────────────────

/** Commits on the workstream branch that base doesn't have (`base..HEAD`), newest
 *  first, each with its own diff stats. Never throws — failures land in `error`. */
export async function reviewCommits(dir: string, base?: string): Promise<ReviewCommitsResult> {
  const resolvedBase = base ?? (await resolveDefaultBase(dir));
  const result: ReviewCommitsResult = { base: resolvedBase, commits: [] };
  const invalid = await verifyRepoAndBase(dir, resolvedBase);
  if (invalid) {
    result.error = invalid;
    return result;
  }
  const log = await runGit(dir, [
    'log',
    '--numstat',
    `--format=${LOG_FORMAT}`,
    `${resolvedBase}..HEAD`,
  ]);
  if (!log.ok) {
    result.error = log.error;
    return result;
  }
  result.commits = parseCommitLog(log.stdout);
  return result;
}

/** The merge-base of base and HEAD — the same baseline `base...HEAD` uses, computed
 *  explicitly so the working tree can be diffed against it directly (covering
 *  committed + uncommitted in one diff) without the base's own advances ever reading
 *  as this workstream's changes. */
async function mergeBase(dir: string, base: string): Promise<GitResult> {
  const result = await runGit(dir, ['merge-base', base, 'HEAD']);
  return result.ok ? { ok: true, stdout: result.stdout.trim() } : result;
}

/** Line count of an untracked file (its entire content is an addition). Binary or
 *  unreadable files count 0 — the entry's presence is the signal. */
async function countLines(dir: string, file: string): Promise<number> {
  try {
    const buf = await fs.readFile(path.join(dir, file));
    if (buf.length === 0) return 0;
    if (buf.subarray(0, 8000).includes(0)) return 0; // git's own binary heuristic
    let lines = 0;
    for (const byte of buf) if (byte === 10) lines += 1;
    return buf[buf.length - 1] === 10 ? lines : lines + 1;
  } catch {
    return 0;
  }
}

/** Per-file diff stats vs base — committed (branch vs merge-base) and uncommitted
 *  (working tree, incl. untracked files) combined into one list. Never throws. */
export async function reviewFiles(dir: string, base?: string): Promise<ReviewFilesResult> {
  const resolvedBase = base ?? (await resolveDefaultBase(dir));
  const result: ReviewFilesResult = { base: resolvedBase, files: [] };
  const invalid = await verifyRepoAndBase(dir, resolvedBase);
  if (invalid) {
    result.error = invalid;
    return result;
  }
  const mb = await mergeBase(dir, resolvedBase);
  if (!mb.ok) {
    result.error = mb.error;
    return result;
  }

  // Diffing the working tree against the merge-base folds committed + uncommitted
  // changes into one net view (a change committed then reverted uncommitted nets out).
  const numstat = await runGit(dir, ['diff', '--numstat', '-z', '-M', mb.stdout]);
  if (!numstat.ok) {
    result.error = numstat.error;
    return result;
  }
  const nameStatus = await runGit(dir, ['diff', '--name-status', '-z', '-M', mb.stdout]);
  if (!nameStatus.ok) {
    result.error = nameStatus.error;
    return result;
  }
  // -uall lists untracked files individually (not collapsed to their directory).
  const porcelain = await runGit(dir, ['status', '--porcelain', '-z', '-uall']);
  if (!porcelain.ok) {
    result.error = porcelain.error;
    return result;
  }

  const stats = new Map(parseNumstatZ(numstat.stdout).map((entry) => [entry.path, entry]));
  const { uncommitted, untracked } = parsePorcelainZ(porcelain.stdout);

  for (const entry of parseNameStatusZ(nameStatus.stdout)) {
    const stat = stats.get(entry.path);
    result.files.push({
      path: entry.path,
      additions: stat?.additions ?? 0,
      deletions: stat?.deletions ?? 0,
      status: entry.status,
      uncommitted: uncommitted.has(entry.path),
      ...(entry.renamedFrom ? { renamedFrom: entry.renamedFrom } : {}),
    });
  }

  // Untracked files never appear in `git diff` — append them as uncommitted additions.
  const seen = new Set(result.files.map((file) => file.path));
  for (const filePath of untracked) {
    if (seen.has(filePath)) continue;
    result.files.push({
      path: filePath,
      additions: await countLines(dir, filePath),
      deletions: 0,
      status: 'added',
      uncommitted: true,
    });
  }
  return result;
}

/** `git diff --no-index` exits 1 when the files differ — that IS the diff here, so
 *  exit 1 with stdout is success. Used for untracked files, which plain diff skips. */
async function noIndexDiff(dir: string, file: string): Promise<GitResult> {
  try {
    const { stdout } = await execFile(
      'git',
      ['-C', dir, 'diff', '--no-index', '--', '/dev/null', file],
      { maxBuffer: MAX_BUFFER },
    );
    return { ok: true, stdout };
  } catch (err) {
    const failure = err as { code?: unknown; stdout?: unknown };
    if (failure.code === 1 && typeof failure.stdout === 'string') {
      return { ok: true, stdout: failure.stdout };
    }
    return { ok: false, error: errText(err) };
  }
}

/** The unified patch for ONE changed file vs base (committed + working tree),
 *  capped at {@link DIFF_CAP} (`truncated` set when cut). Traversal-proof by
 *  construction: `file` must exactly match a path git itself reported via
 *  {@link reviewFiles} — anything else (including `../` escapes) is `unknownPath`,
 *  and no git/fs call ever receives the raw caller path otherwise. Never throws. */
export async function reviewDiff(
  dir: string,
  base: string | undefined,
  file: string,
): Promise<ReviewDiffResult> {
  const files = await reviewFiles(dir, base);
  const result: ReviewDiffResult = { base: files.base, path: file, patch: '', truncated: false };
  if (files.error) {
    result.error = files.error;
    return result;
  }
  const entry = files.files.find((candidate) => candidate.path === file);
  if (!entry) {
    result.unknownPath = true;
    result.error = `path was not reported by git as changed vs ${files.base}: ${file}`;
    return result;
  }

  const mb = await mergeBase(dir, files.base);
  if (!mb.ok) {
    result.error = mb.error;
    return result;
  }
  // For a rename, limit the diff to both sides so -M can pair them into one hunk.
  const pathspecs = entry.renamedFrom ? [entry.renamedFrom, entry.path] : [entry.path];
  let patch = await runGit(dir, ['diff', '-M', mb.stdout, '--', ...pathspecs]);
  if (patch.ok && patch.stdout === '') {
    // Nothing from plain diff but git reported the file ⇒ it is untracked.
    patch = await noIndexDiff(dir, entry.path);
  }
  if (!patch.ok) {
    result.error = patch.error;
    return result;
  }
  // Cap by string length (~bytes for ASCII patches) — approximate is fine here.
  if (patch.stdout.length > DIFF_CAP) {
    result.patch = patch.stdout.slice(0, DIFF_CAP);
    result.truncated = true;
  } else {
    result.patch = patch.stdout;
  }
  return result;
}
