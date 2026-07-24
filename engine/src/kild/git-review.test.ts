import { afterAll, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import {
  DIFF_CAP,
  parseCommitLog,
  parseNameStatusZ,
  parseNumstatZ,
  parsePorcelainZ,
  reviewCommits,
  reviewDiff,
  reviewFiles,
} from './git-review.ts';

const execFile = promisify(execFileCb);

const tmpDirs: string[] = [];

afterAll(() => {
  for (const dir of tmpDirs) fs.rmSync(dir, { recursive: true, force: true });
});

function mkTmp(prefix: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  tmpDirs.push(dir);
  return dir;
}

async function git(dir: string, args: string[]): Promise<void> {
  await execFile('git', ['-C', dir, ...args]);
}

// Commit with a fixed identity so the temp repo doesn't depend on the host's git config.
async function commit(dir: string, message: string): Promise<void> {
  await execFile('git', [
    '-C',
    dir,
    '-c',
    'user.email=t@t',
    '-c',
    'user.name=t',
    'commit',
    '-m',
    message,
  ]);
}

// A fresh repo on `main` with one committed file.
async function initRepo(): Promise<string> {
  const dir = mkTmp('kild-git-review-');
  await git(dir, ['init', '-b', 'main']);
  fs.writeFileSync(path.join(dir, 'README.md'), 'hello\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'initial');
  return dir;
}

// ── reviewCommits ─────────────────────────────────────────────────────────────

test('commits vs base come newest-first with per-commit stats', async () => {
  const dir = await initRepo();
  await git(dir, ['checkout', '-b', 'feature']);
  fs.writeFileSync(path.join(dir, 'a.txt'), 'one\ntwo\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'add a');
  fs.writeFileSync(path.join(dir, 'a.txt'), 'one\n');
  fs.writeFileSync(path.join(dir, 'b.txt'), 'b\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'trim a, add b');

  const result = await reviewCommits(dir, 'main');

  expect(result.error).toBeUndefined();
  expect(result.base).toBe('main');
  expect(result.commits).toHaveLength(2);
  const [second, first] = result.commits;
  expect(second?.subject).toBe('trim a, add b');
  expect(second?.author).toBe('t');
  expect(second?.sha).toMatch(/^[0-9a-f]{40}$/);
  expect(second?.ts).toBeGreaterThan(1_600_000_000_000); // epoch millis, not seconds
  expect(second?.filesChanged).toBe(2);
  expect(second?.additions).toBe(1); // b.txt line
  expect(second?.deletions).toBe(1); // trimmed a.txt line
  expect(first?.subject).toBe('add a');
  expect(first?.filesChanged).toBe(1);
  expect(first?.additions).toBe(2);
  expect(first?.deletions).toBe(0);
});

test('no commits ahead of base yields an empty list, no error', async () => {
  const dir = await initRepo();
  const result = await reviewCommits(dir, 'main');
  expect(result.error).toBeUndefined();
  expect(result.commits).toEqual([]);
});

test('commits: a missing base ref is an error object, not a crash', async () => {
  const dir = await initRepo();
  const result = await reviewCommits(dir, 'does-not-exist');
  expect(result.error).toBe('base ref not found: does-not-exist');
  expect(result.commits).toEqual([]);
});

test('commits: a non-git directory is an error object, not a crash', async () => {
  const dir = mkTmp('kild-git-review-nogit-');
  const result = await reviewCommits(dir, 'main');
  expect(result.error).toBeDefined();
  expect(result.commits).toEqual([]);
});

// ── reviewFiles ───────────────────────────────────────────────────────────────

test('files combine committed, uncommitted, and untracked changes vs base', async () => {
  const dir = await initRepo();
  await git(dir, ['checkout', '-b', 'feature']);
  fs.writeFileSync(path.join(dir, 'committed.txt'), 'one\ntwo\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'add committed');
  fs.writeFileSync(path.join(dir, 'README.md'), 'hello\nedited\n'); // uncommitted edit
  fs.writeFileSync(path.join(dir, 'untracked.txt'), 'u1\nu2\nu3\n'); // never added

  const result = await reviewFiles(dir, 'main');

  expect(result.error).toBeUndefined();
  const byPath = new Map(result.files.map((file) => [file.path, file]));
  expect(byPath.get('committed.txt')).toEqual({
    path: 'committed.txt',
    additions: 2,
    deletions: 0,
    status: 'added',
    uncommitted: false,
  });
  expect(byPath.get('README.md')).toEqual({
    path: 'README.md',
    additions: 1,
    deletions: 0,
    status: 'modified',
    uncommitted: true,
  });
  expect(byPath.get('untracked.txt')).toEqual({
    path: 'untracked.txt',
    additions: 3,
    deletions: 0,
    status: 'added',
    uncommitted: true,
  });
});

test('files report deletions and renames with the pre-rename path', async () => {
  const dir = await initRepo();
  fs.writeFileSync(path.join(dir, 'gone.txt'), 'bye\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'add gone');
  await git(dir, ['checkout', '-b', 'feature']);
  await git(dir, ['rm', '-q', 'gone.txt']);
  await git(dir, ['mv', 'README.md', 'RENAMED.md']);
  await commit(dir, 'delete + rename');

  const result = await reviewFiles(dir, 'main');

  expect(result.error).toBeUndefined();
  const byPath = new Map(result.files.map((file) => [file.path, file]));
  expect(byPath.get('gone.txt')).toMatchObject({ status: 'deleted', deletions: 1 });
  expect(byPath.get('RENAMED.md')).toMatchObject({
    status: 'renamed',
    renamedFrom: 'README.md',
  });
});

test("files never include the base's own advances (merge-base semantics)", async () => {
  const dir = await initRepo();
  await git(dir, ['checkout', '-b', 'feature']);
  fs.writeFileSync(path.join(dir, 'mine.txt'), 'mine\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'mine');
  // Advance main independently — that change is NOT this workstream's work.
  await git(dir, ['checkout', 'main']);
  fs.writeFileSync(path.join(dir, 'theirs.txt'), 'theirs\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'theirs');
  await git(dir, ['checkout', 'feature']);

  const result = await reviewFiles(dir, 'main');

  expect(result.error).toBeUndefined();
  expect(result.files.map((file) => file.path)).toEqual(['mine.txt']);
});

test('files: a missing base ref is an error object, not a crash', async () => {
  const dir = await initRepo();
  const result = await reviewFiles(dir, 'does-not-exist');
  expect(result.error).toBe('base ref not found: does-not-exist');
  expect(result.files).toEqual([]);
});

// ── reviewDiff ────────────────────────────────────────────────────────────────

test('diff returns one unified patch covering committed + working-tree changes', async () => {
  const dir = await initRepo();
  await git(dir, ['checkout', '-b', 'feature']);
  fs.writeFileSync(path.join(dir, 'README.md'), 'hello\ncommitted\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'committed line');
  fs.writeFileSync(path.join(dir, 'README.md'), 'hello\ncommitted\nuncommitted\n');

  const result = await reviewDiff(dir, 'main', 'README.md');

  expect(result.error).toBeUndefined();
  expect(result.truncated).toBe(false);
  expect(result.patch).toContain('diff --git');
  expect(result.patch).toContain('+committed');
  expect(result.patch).toContain('+uncommitted');
});

test('diff covers an untracked file via no-index', async () => {
  const dir = await initRepo();
  fs.writeFileSync(path.join(dir, 'fresh.txt'), 'brand new\n');

  const result = await reviewDiff(dir, 'main', 'fresh.txt');

  expect(result.error).toBeUndefined();
  expect(result.patch).toContain('+brand new');
});

test('diff refuses a path git did not report (traversal guard)', async () => {
  const dir = await initRepo();
  for (const evil of ['../../etc/passwd', '/etc/passwd', 'nope.txt']) {
    const result = await reviewDiff(dir, 'main', evil);
    expect(result.unknownPath).toBe(true);
    expect(result.error).toContain('not reported by git');
    expect(result.patch).toBe('');
  }
});

test('diff larger than the cap is truncated and flagged', async () => {
  const dir = await initRepo();
  fs.writeFileSync(path.join(dir, 'big.txt'), 'x-line-of-payload\n'.repeat(20_000)); // ~360 KB

  const result = await reviewDiff(dir, 'main', 'big.txt');

  expect(result.error).toBeUndefined();
  expect(result.truncated).toBe(true);
  expect(result.patch.length).toBe(DIFF_CAP);
});

// ── pure parsers ──────────────────────────────────────────────────────────────

test('parseCommitLog handles binary numstat entries and empty commits', () => {
  const stdout =
    '\x1eaaa\x1fbinary drop\x1falice\x1f1700000000\n\n-\t-\timage.png\n5\t0\tcode.ts\n' +
    '\x1ebbb\x1fempty commit\x1fbob\x1f1700000100\n';
  const commits = parseCommitLog(stdout);
  expect(commits).toHaveLength(2);
  expect(commits[0]).toEqual({
    sha: 'aaa',
    subject: 'binary drop',
    author: 'alice',
    ts: 1_700_000_000_000,
    filesChanged: 2,
    additions: 5,
    deletions: 0,
  });
  expect(commits[1]).toMatchObject({ sha: 'bbb', filesChanged: 0, additions: 0, deletions: 0 });
});

test('parseNumstatZ decodes plain, binary, and rename entries', () => {
  const stdout = '3\t1\tplain.ts\0-\t-\tbin.png\x005\t0\t\0old.ts\0new.ts\0';
  expect(parseNumstatZ(stdout)).toEqual([
    { path: 'plain.ts', additions: 3, deletions: 1 },
    { path: 'bin.png', additions: 0, deletions: 0 },
    { path: 'new.ts', additions: 5, deletions: 0 },
  ]);
});

test('parseNameStatusZ decodes add/modify/delete/rename', () => {
  const stdout = 'A\0new.ts\0M\0mod.ts\0D\0gone.ts\0R100\0old.ts\0moved.ts\0';
  expect(parseNameStatusZ(stdout)).toEqual([
    { path: 'new.ts', status: 'added' },
    { path: 'mod.ts', status: 'modified' },
    { path: 'gone.ts', status: 'deleted' },
    { path: 'moved.ts', status: 'renamed', renamedFrom: 'old.ts' },
  ]);
});

test('parsePorcelainZ separates untracked from tracked changes and skips rename origins', () => {
  const stdout = ' M edited.ts\0?? fresh.ts\0R  renamed.ts\0orig.ts\0';
  const { uncommitted, untracked } = parsePorcelainZ(stdout);
  expect(untracked).toEqual(['fresh.ts']);
  expect(uncommitted.has('edited.ts')).toBe(true);
  expect(uncommitted.has('renamed.ts')).toBe(true);
  expect(uncommitted.has('orig.ts')).toBe(false);
});
