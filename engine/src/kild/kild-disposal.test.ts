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
    base: { base: 'main', source: 'explicit' },
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
    base: { base: 'main', source: 'explicit' },
    inUse: false,
  });
  expect(assessment.ok).toBe(true);
  if (!assessment.ok) return;
  expect(assessment.discarded).toEqual(['a.txt']);
});

test('an undeterminable discard list says so, instead of reporting nothing lost', async () => {
  // The list is the whole of what the operator is told they are about to destroy, and it
  // used to be `.catch(() => [])` — so a git failure rendered as a confident "nothing will
  // be lost" at the exact moment force-removal was about to lose it. Reproduced by deleting
  // the tree's `.git` pointer: git can no longer answer, but the guard above already did.
  const wt = await ensureWorktree(repo, 'unreadable', 'main');
  writeFileSync(path.join(wt.path, 'work.txt'), 'never committed\n');
  rmSync(path.join(wt.path, '.git'), { recursive: true, force: true });

  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/unreadable',
    base: { base: 'main', source: 'explicit' },
    inUse: false,
    force: true, // past the commits guard, which refuses `undetermined` on its own
  });
  expect(assessment.ok).toBe(true);
  if (!assessment.ok) return;
  expect(assessment.discarded).toEqual([]);
  // …but empty means UNKNOWN here, and the caller can tell the two apart.
  expect(assessment.discardedError).toBeTruthy();
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
    base: { base: 'main', source: 'explicit' },
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
    base: { base: 'main', source: 'explicit' },
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
    base: { base: 'no-such-base', source: 'explicit' },
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
      base: { base: 'no-such-base', source: 'explicit' },
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

test('a refusal built on a GUESSED base says so, and one on a chosen base does not', async () => {
  // The measured failure: 116 trees held behind "carries 365 commits not in main". True, and
  // useless — that repo's default is `dev`, so every tree was hundreds of commits ahead of a
  // branch it had never forked from, by construction. The refusal named the base. What it
  // could not say was that nobody had chosen it, and that cost four measurement passes and
  // 5.4 GB. The count is only evidence of unlanded work if the base is.
  const wt = await ensureWorktree(repo, 'guessed', 'main');
  writeFileSync(path.join(wt.path, 'a.txt'), 'authored\n');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', 'real work');

  // No `base` given, and this repo has no origin/HEAD → the literal fallback.
  const guessed = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/guessed',
    inUse: false,
  });
  expect(guessed.ok).toBe(false);
  if (guessed.ok) return;
  expect(guessed.baseSource).toBe('fallback');
  expect(guessed.base).toBe('main');
  expect(guessed.message).toContain('nothing configured a base');
  expect(guessed.message).toContain('.kild/config.json');

  // Named explicitly: the same refusal, with no hedge, because somebody chose the branch.
  const chosen = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/guessed',
    base: { base: 'main', source: 'explicit' },
    inUse: false,
  });
  expect(chosen.ok).toBe(false);
  if (chosen.ok) return;
  expect(chosen.baseSource).toBe('explicit');
  expect(chosen.message).not.toContain('this is a guess');
  expect(chosen.message).toContain('carries 1 commit not in main');
});

test('a base cached in origin/HEAD is labelled as the cache it is', async () => {
  // origin/HEAD is written at clone time and never refreshed, so it goes stale silently when
  // the remote's default moves. Better than a literal guess; still not a fact anyone asserted.
  await git('remote', 'add', 'origin', repo);
  await git('symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/main');
  await git('update-ref', 'refs/remotes/origin/main', 'main');

  const wt = await ensureWorktree(repo, 'cached', 'main');
  writeFileSync(path.join(wt.path, 'a.txt'), 'authored\n');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', 'real work');

  const assessment = await assessDisposal({
    repo,
    dir: wt.path,
    branch: 'kild/cached',
    inUse: false,
  });
  expect(assessment.ok).toBe(false);
  if (assessment.ok) return;
  expect(assessment.baseSource).toBe('origin-head');
  expect(assessment.message).toContain('origin/HEAD');
  expect(assessment.message).toContain('caches at clone time');
});
