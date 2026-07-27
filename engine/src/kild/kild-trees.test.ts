import { afterEach, beforeEach, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { kildTrees, orphanTrees } from './kild-trees.ts';
import { ensureWorktree } from './worktree.ts';

/**
 * The inventory git owns. The registry only knows the kilds THIS engine process created, so
 * a tree from an earlier run has no record, no id, and nothing could address it — that is
 * the stranding these functions exist to end.
 */
const execFile = promisify(execFileCb);

let repo: string;
let home: string;
let prevHome: string | undefined;

const git = (...args: string[]) => execFile('git', ['-C', repo, ...args]);

beforeEach(async () => {
  repo = mkdtempSync(path.join(tmpdir(), 'kild-trees-repo-'));
  home = mkdtempSync(path.join(tmpdir(), 'kild-trees-home-'));
  prevHome = process.env.KILD_HOME;
  process.env.KILD_HOME = home;
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

test('every kild/* worktree is reported with the repo it belongs to', async () => {
  const a = await ensureWorktree(repo, 'fix-auth', 'main');
  const b = await ensureWorktree(repo, 'add-logs', 'main');
  const trees = await kildTrees([repo]);
  expect(trees.map((t) => t.worktree).sort()).toEqual(['add-logs', 'fix-auth']);
  expect(trees.every((t) => t.repo === repo)).toBe(true);
  expect(trees.find((t) => t.worktree === 'fix-auth')).toMatchObject({
    branch: 'kild/fix-auth',
    path: a.path,
  });
  expect(trees.find((t) => t.worktree === 'add-logs')?.path).toBe(b.path);
});

test('a worktree that is NOT a kild is never claimed — nor is the main checkout', async () => {
  await ensureWorktree(repo, 'mine', 'main');
  // A tree the human made on their own branch. kild did not create it and must not list it
  // (listing it would make it disposable through a kild verb).
  const theirs = path.join(home, 'their-tree');
  await git('worktree', 'add', '-q', '-b', 'feature/theirs', theirs, 'main');

  const trees = await kildTrees([repo]);
  expect(trees.map((t) => t.worktree)).toEqual(['mine']);
  expect(trees.some((t) => t.path === repo)).toBe(false);
});

test('a repo git cannot read contributes nothing rather than failing the inventory', async () => {
  await ensureWorktree(repo, 'real', 'main');
  const notARepo = mkdtempSync(path.join(tmpdir(), 'kild-trees-plain-'));
  const trees = await kildTrees([notARepo, repo, path.join(tmpdir(), 'kild-trees-gone-xyz')]);
  expect(trees.map((t) => t.worktree)).toEqual(['real']);
  rmSync(notARepo, { recursive: true, force: true });
});

test('the same repo listed twice yields each tree once', async () => {
  await ensureWorktree(repo, 'once', 'main');
  expect((await kildTrees([repo, repo])).map((t) => t.worktree)).toEqual(['once']);
});

test('orphans are the trees no live kild occupies', async () => {
  await ensureWorktree(repo, 'live-one', 'main');
  await ensureWorktree(repo, 'abandoned', 'main');
  const trees = await kildTrees([repo]);
  expect(orphanTrees(trees, new Set(['live-one'])).map((t) => t.worktree)).toEqual(['abandoned']);
  expect(
    orphanTrees(trees, new Set())
      .map((t) => t.worktree)
      .sort(),
  ).toEqual(['abandoned', 'live-one']);
});
