import { afterEach, beforeEach, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { assessDisposal, removeKildTree } from './kild-disposal.ts';
import { ensureWorktree, worktreePath } from './worktree.ts';

/**
 * Real-git tests for the ONE guard disposal answers: **authored commits, not dirt.**
 *
 * The measured failure this replaces: provisioning wrote files into every tree before any
 * agent ran, `changedFiles()` counted them, removal refused as "dirty", and 116 trees became
 * permanently unreclaimable with nothing reporting it. So litter must NOT refuse, commits
 * MUST, and every path has to say which.
 */
const execFile = promisify(execFileCb);

let repo: string;
let home: string;
let prevHome: string | undefined;

const git = (...args: string[]) => execFile('git', ['-C', repo, ...args]);
const gitIn = (dir: string, ...args: string[]) => execFile('git', ['-C', dir, ...args]);

beforeEach(async () => {
  repo = mkdtempSync(path.join(tmpdir(), 'kild-disposal-repo-'));
  home = mkdtempSync(path.join(tmpdir(), 'kild-disposal-home-'));
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

test('a tree with ONLY untracked litter is disposable, and the litter is named', async () => {
  const wt = await ensureWorktree(repo, 'litter', 'main');
  // Exactly the provisioning shape: files written into the tree before any agent ran.
  writeFileSync(path.join(wt.path, '.archon.yaml'), 'provisioned: true\n');
  writeFileSync(path.join(wt.path, 'notes.txt'), 'scratch\n');

  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/litter',
    base: 'main',
    inUse: false,
  });
  expect(assessment.ok).toBe(true);
  if (!assessment.ok) return;
  expect(assessment.commits).toBe(0);
  expect(assessment.forced).toBe(false);
  // Never hidden: the caller can show exactly what removal costs.
  expect(assessment.discarded.sort()).toEqual(['.archon.yaml', 'notes.txt']);
});

test('uncommitted changes to TRACKED files are not a refusal either', async () => {
  const wt = await ensureWorktree(repo, 'tracked', 'main');
  writeFileSync(path.join(wt.path, 'a.txt'), 'one\n');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', 'a');
  // That commit IS authored work, so land it first — the point here is the working tree.
  await git('merge', '-q', '--no-ff', '-m', 'land', 'kild/tracked');
  writeFileSync(path.join(wt.path, 'a.txt'), 'edited but never committed\n');

  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/tracked',
    base: 'main',
    inUse: false,
  });
  expect(assessment.ok).toBe(true);
  if (!assessment.ok) return;
  expect(assessment.discarded).toEqual(['a.txt']);
});

test('a branch carrying commits base does not have is REFUSED, with the count and tip', async () => {
  const wt = await ensureWorktree(repo, 'authored', 'main');
  writeFileSync(path.join(wt.path, 'work.ts'), 'real work\n');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', 'the work');

  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/authored',
    base: 'main',
    inUse: false,
  });
  expect(assessment.ok).toBe(false);
  if (assessment.ok) return;
  expect(assessment.code).toBe('authored');
  expect(assessment.commits).toBe(1);
  expect(assessment.tip).toMatch(/^[0-9a-f]{7}$/);
  expect(assessment.message).toContain('1 commit not in main');
  expect(assessment.message).toContain('land it');
  expect(existsSync(wt.path)).toBe(true); // assessing removes nothing
});

test('force overrides the authored refusal — the branch and its commits survive it', async () => {
  const wt = await ensureWorktree(repo, 'forced', 'main');
  writeFileSync(path.join(wt.path, 'work.ts'), 'real work\n');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', 'the work');
  const sha = (await gitIn(wt.path, 'rev-parse', 'HEAD')).stdout.trim();

  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/forced',
    base: 'main',
    inUse: false,
    force: true,
  });
  expect(assessment).toMatchObject({ ok: true, commits: 1, forced: true });

  await removeKildTree(repo, wt.path);
  expect(existsSync(wt.path)).toBe(false);
  // The whole reason force is safe: the branch still points at the work.
  expect((await git('rev-parse', 'kild/forced')).stdout.trim()).toBe(sha);
});

test('a tree a live agent is working in is refused before anything else is even checked', async () => {
  const assessment = await assessDisposal({
    repo,
    dir: worktreePath('never-created'),
    branch: 'kild/never-created',
    inUse: true,
  });
  expect(assessment).toMatchObject({ ok: false, code: 'in_use' });
  if (assessment.ok) return;
  expect(assessment.message).toContain('stop it first');
});

test('a path that is no registered worktree is not_found, never a silent success', async () => {
  const assessment = await assessDisposal({
    repo,
    dir: worktreePath('missing'),
    branch: 'kild/missing',
    inUse: false,
  });
  expect(assessment).toMatchObject({ ok: false, code: 'not_found' });
});

test('a base that cannot be resolved is undetermined, and says force is the way through', async () => {
  const wt = await ensureWorktree(repo, 'nobase', 'main');
  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/nobase',
    base: 'no-such-base',
    inUse: false,
  });
  expect(assessment).toMatchObject({ ok: false, code: 'undetermined' });
  if (assessment.ok) return;
  expect(assessment.message).toContain('force');
  // …and force does get through it.
  expect(
    await assessDisposal({
      repo,
      dir: wt.path,
      branch: 'kild/nobase',
      base: 'no-such-base',
      inUse: false,
      force: true,
    }),
  ).toMatchObject({ ok: true });
});

test('removal frees the tree and keeps the branch — disposal never deletes work', async () => {
  const wt = await ensureWorktree(repo, 'reclaim', 'main');
  writeFileSync(path.join(wt.path, 'litter.txt'), 'x');
  await removeKildTree(repo, wt.path);
  expect(existsSync(wt.path)).toBe(false);
  expect((await git('branch')).stdout).toContain('kild/reclaim');
});
