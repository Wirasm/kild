import { afterEach, beforeEach, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { landMerge, landPlan } from './kild-land.ts';
import { ensureWorktree } from './worktree.ts';

/**
 * Landing, against real git. Two properties matter most:
 *
 * - the dry run **touches nothing** (no ref, no index, no working tree, in the kild's tree
 *   OR the main checkout) — it has to be safe to call on every render;
 * - a land that did not happen never reports as one, and a land that did names its sha.
 */
const execFile = promisify(execFileCb);

let repo: string;
let home: string;
let prevHome: string | undefined;

const git = (...args: string[]) => execFile('git', ['-C', repo, ...args]);
const gitIn = (dir: string, ...args: string[]) => execFile('git', ['-C', dir, ...args]);

beforeEach(async () => {
  repo = mkdtempSync(path.join(tmpdir(), 'kild-land-repo-'));
  home = mkdtempSync(path.join(tmpdir(), 'kild-land-home-'));
  prevHome = process.env.KILD_HOME;
  process.env.KILD_HOME = home;
  await execFile('git', ['-C', repo, 'init', '-q', '-b', 'main']);
  await git('config', 'user.email', 't@t');
  await git('config', 'user.name', 't');
  writeFileSync(path.join(repo, 'README.md'), 'base\n');
  await git('add', '.');
  await git('commit', '-q', '-m', 'init');
});

afterEach(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  rmSync(repo, { recursive: true, force: true });
  rmSync(home, { recursive: true, force: true });
});

/** A kild tree with one commit on it. */
async function kildWithWork(name: string, file = 'work.ts', body = 'the work\n') {
  const wt = await ensureWorktree(repo, name, 'main');
  writeFileSync(path.join(wt.path, file), body);
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', `work on ${name}`);
  return wt;
}

/** Everything a land must not disturb until it is asked to. */
async function snapshot(dir: string) {
  return {
    head: (await gitIn(dir, 'rev-parse', 'HEAD')).stdout.trim(),
    refs: (await gitIn(dir, 'show-ref')).stdout,
    status: (await gitIn(dir, 'status', '--porcelain')).stdout,
  };
}

test('the dry run reports what would land and TOUCHES NOTHING', async () => {
  const wt = await kildWithWork('dry');
  writeFileSync(path.join(wt.path, 'litter.txt'), 'uncommitted\n'); // must survive untouched
  const beforeTree = await snapshot(wt.path);
  const beforeRepo = await snapshot(repo);

  const plan = await landPlan(wt.path, 'main');
  expect(plan.wouldMerge).toBe(true);
  expect(plan.merged).toBe(false);
  expect(plan.sha).toBeUndefined();
  expect(plan.error).toBeUndefined();
  expect(plan.branch).toBe('kild/dry');
  expect(plan.base).toBe('main');
  expect(plan.commits.map((c) => c.subject)).toEqual(['work on dry']);
  expect(plan.files).toEqual(['work.ts']);
  expect(plan.collides).toEqual([]);

  expect(await snapshot(wt.path)).toEqual(beforeTree);
  expect(await snapshot(repo)).toEqual(beforeRepo);
});

test('the dry run names the colliding files and would not merge', async () => {
  const wt = await kildWithWork('collide', 'README.md', 'kild side\n');
  writeFileSync(path.join(repo, 'README.md'), 'main side\n');
  await git('commit', '-q', '-am', 'main moves');
  const beforeRepo = await snapshot(repo);

  const plan = await landPlan(wt.path, 'main');
  expect(plan.wouldMerge).toBe(false);
  expect(plan.collides).toEqual(['README.md']);
  expect(plan.error).toContain('conflicts in 1 file');
  expect(await snapshot(repo)).toEqual(beforeRepo);
});

test('a kild with nothing committed would not land, and says exactly that', async () => {
  const wt = await ensureWorktree(repo, 'empty', 'main');
  writeFileSync(path.join(wt.path, 'only-litter.txt'), 'x');
  const plan = await landPlan(wt.path, 'main');
  expect(plan.wouldMerge).toBe(false);
  expect(plan.error).toBe('nothing committed on kild/empty vs main');
});

test('a kild that ran in the checkout has no branch to land', async () => {
  const plan = await landPlan(repo, 'main');
  expect(plan.wouldMerge).toBe(false);
  expect(plan.error).toContain('no branch to land');
});

test('landing merges into base and reports the merge sha', async () => {
  const wt = await kildWithWork('ship');
  const result = await landMerge(repo, wt.path, 'main');
  expect(result.merged).toBe(true);
  expect(result.sha).toMatch(/^[0-9a-f]{40}$/);
  // The base really carries the work now, at exactly the sha reported.
  expect((await git('rev-parse', 'HEAD')).stdout.trim()).toBe(result.sha);
  expect((await git('log', '--format=%s', '-1')).stdout.trim()).toBe(
    'kild: land kild/ship into main',
  );
  expect((await git('branch', '--merged', 'main')).stdout).toContain('kild/ship');
  // And a dry run afterwards agrees there is nothing left to land.
  expect((await landPlan(wt.path, 'main')).wouldMerge).toBe(false);
});

test('landing refuses when base is not checked out in the repo, and merges nothing', async () => {
  const wt = await kildWithWork('elsewhere');
  await git('checkout', '-q', '-b', 'side');
  const before = await snapshot(repo);

  const result = await landMerge(repo, wt.path, 'main');
  expect(result.merged).toBe(false);
  expect(result.error).toContain('is on side, not main');
  expect(await snapshot(repo)).toEqual(before);
});

test('landing refuses on a dirty main checkout rather than entangling the merge', async () => {
  const wt = await kildWithWork('dirty-base');
  writeFileSync(path.join(repo, 'README.md'), 'edited in the checkout\n');
  const before = await snapshot(repo);

  const result = await landMerge(repo, wt.path, 'main');
  expect(result.merged).toBe(false);
  expect(result.error).toContain('uncommitted changes');
  expect(await snapshot(repo)).toEqual(before);
});

test('a conflicting land is refused before git is asked to merge', async () => {
  const wt = await kildWithWork('bad-merge', 'README.md', 'kild side\n');
  writeFileSync(path.join(repo, 'README.md'), 'main side\n');
  await git('commit', '-q', '-am', 'main moves');
  const before = await snapshot(repo);

  const result = await landMerge(repo, wt.path, 'main');
  expect(result.merged).toBe(false);
  expect(result.collides).toEqual(['README.md']);
  // No half-finished merge left behind for someone else to discover.
  expect(await snapshot(repo)).toEqual(before);
});
