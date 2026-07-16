import { afterEach, beforeEach, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import {
  ensureWorktree,
  forceRemoveWorktree,
  pruneMergedWorktrees,
  removeWorktree,
  worktreePath,
} from './worktree.ts';

// Real-git tests for the two data-loss/safety paths: merge-prune must never destroy
// uncommitted work or in-use trees, and ensureWorktree must attach (never reset).
const execFile = promisify(execFileCb);

let repo: string;
let home: string;
let prevHome: string | undefined;

const git = (...args: string[]) => execFile('git', ['-C', repo, ...args]);
const gitIn = (dir: string, ...args: string[]) => execFile('git', ['-C', dir, ...args]);

beforeEach(async () => {
  repo = mkdtempSync(path.join(tmpdir(), 'kild-wt-repo-'));
  home = mkdtempSync(path.join(tmpdir(), 'kild-wt-home-'));
  prevHome = process.env.KILD_HOME;
  process.env.KILD_HOME = home; // worktreePath() roots under $KILD_HOME/worktrees
  await execFile('git', ['-C', repo, 'init', '-q', '-b', 'main']);
  await git('config', 'user.email', 't@t');
  await git('config', 'user.name', 't');
  await git('commit', '-q', '--allow-empty', '-m', 'init');
});

afterEach(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  rmSync(repo, { recursive: true, force: true });
  rmSync(home, { recursive: true, force: true });
});

test('prune removes a clean merged worktree and deletes its branch', async () => {
  const wt = await ensureWorktree(repo, 'merged'); // kild/merged == base ⇒ merged, clean
  const pruned = await pruneMergedWorktrees(repo);
  expect(pruned).toContain('merged');
  expect(existsSync(wt.path)).toBe(false);
  expect((await git('branch')).stdout).not.toContain('kild/merged');
});

test('prune skips an unmerged worktree (commits ahead of base)', async () => {
  const wt = await ensureWorktree(repo, 'ahead');
  await gitIn(wt.path, 'commit', '-q', '--allow-empty', '-m', 'ahead');
  const pruned = await pruneMergedWorktrees(repo);
  expect(pruned).not.toContain('ahead');
  expect(existsSync(wt.path)).toBe(true);
});

test('prune keep-set protects an in-use worktree', async () => {
  const wt = await ensureWorktree(repo, 'keepme');
  const pruned = await pruneMergedWorktrees(repo, new Set(['keepme']));
  expect(pruned).not.toContain('keepme');
  expect(existsSync(wt.path)).toBe(true);
});

test('prune never touches the worktree named after the default branch', async () => {
  const wt = await ensureWorktree(repo, 'main'); // kild/main === kild/<base>
  const pruned = await pruneMergedWorktrees(repo);
  expect(pruned).not.toContain('main');
  expect(existsSync(wt.path)).toBe(true);
});

test('prune PRESERVES a merged worktree with uncommitted/untracked work (no --force)', async () => {
  const wt = await ensureWorktree(repo, 'dirty'); // merged, but…
  writeFileSync(path.join(wt.path, 'UNTRACKED.txt'), 'wip'); // …has untracked work
  const pruned = await pruneMergedWorktrees(repo);
  expect(pruned).not.toContain('dirty');
  expect(existsSync(wt.path)).toBe(true);
  expect(existsSync(path.join(wt.path, 'UNTRACKED.txt'))).toBe(true);
});

test('ensureWorktree attaches without resetting (uncommitted work survives)', async () => {
  const wt = await ensureWorktree(repo, 'attach');
  writeFileSync(path.join(wt.path, 'WIP.txt'), 'x');
  const again = await ensureWorktree(repo, 'attach');
  expect(again.path).toBe(wt.path);
  expect(existsSync(path.join(wt.path, 'WIP.txt'))).toBe(true);
});

test('safe removal removes a clean worktree', async () => {
  const wt = await ensureWorktree(repo, 'clean');
  await expect(removeWorktree(repo, wt.path)).resolves.toEqual({ ok: true });
  expect(existsSync(wt.path)).toBe(false);
});

test('safe removal refuses dirty worktree and previews modified and untracked files', async () => {
  const wt = await ensureWorktree(repo, 'dirty-remove');
  writeFileSync(path.join(wt.path, 'tracked.txt'), 'base');
  await gitIn(wt.path, 'add', 'tracked.txt');
  await gitIn(wt.path, 'commit', '-q', '-m', 'tracked');
  writeFileSync(path.join(wt.path, 'tracked.txt'), 'changed');
  writeFileSync(path.join(wt.path, 'untracked.txt'), 'wip');

  await expect(removeWorktree(repo, wt.path)).resolves.toEqual({
    ok: false,
    code: 'dirty',
    files: expect.arrayContaining(['tracked.txt', 'untracked.txt']),
  });
  expect(existsSync(wt.path)).toBe(true);
});

test('force removal discards a dirty worktree', async () => {
  const wt = await ensureWorktree(repo, 'force-dirty');
  writeFileSync(path.join(wt.path, 'WIP.txt'), 'discard');
  await expect(forceRemoveWorktree(repo, wt.path)).resolves.toEqual({ ok: true });
  expect(existsSync(wt.path)).toBe(false);
});

test('safe removal reports a missing worktree', async () => {
  await expect(removeWorktree(repo, worktreePath('missing'))).resolves.toEqual({
    ok: false,
    code: 'not_found',
  });
});

test('ensureWorktree re-creating a removed worktree preserves the branch commits', async () => {
  const wt = await ensureWorktree(repo, 'persist');
  writeFileSync(path.join(wt.path, 'COMMITTED.txt'), 'keep');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', 'work');
  await removeWorktree(repo, wt.path); // worktree gone; kild/persist branch kept
  expect(existsSync(wt.path)).toBe(false);
  const again = await ensureWorktree(repo, 'persist');
  expect(existsSync(path.join(again.path, 'COMMITTED.txt'))).toBe(true);
});

test('ensureWorktree throws on a stale non-worktree dir (no silent non-isolated cwd)', async () => {
  mkdirSync(worktreePath('stale'), { recursive: true });
  await expect(ensureWorktree(repo, 'stale')).rejects.toThrow();
});
