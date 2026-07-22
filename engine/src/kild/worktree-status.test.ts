import { afterAll, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { workstreamGitStatus } from './worktree-status.ts';

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
  const dir = mkTmp('kild-wt-status-');
  await git(dir, ['init', '-b', 'main']);
  fs.writeFileSync(path.join(dir, 'README.md'), 'hello\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'initial');
  return dir;
}

test('clean repo on the base branch reports no divergence and no changes', async () => {
  const dir = await initRepo();
  const status = await workstreamGitStatus(dir); // default base resolves to main

  expect(status.error).toBeUndefined();
  expect(status.path).toBe(dir);
  expect(status.branch).toBe('main');
  expect(status.base).toBe('main');
  expect(status.ahead).toBe(0);
  expect(status.behind).toBe(0);
  expect(status.dirty).toBe(false);
  expect(status.uncommittedFiles).toBe(0);
  expect(status.changedFiles).toEqual([]);
  expect(status.conflictsWithBase).toBeNull();
});

test('a branch one commit ahead reports ahead 1 and the changed file', async () => {
  const dir = await initRepo();
  await git(dir, ['checkout', '-b', 'feature']);
  fs.writeFileSync(path.join(dir, 'feature.txt'), 'x\n');
  await git(dir, ['add', '.']);
  await commit(dir, 'add feature');

  const status = await workstreamGitStatus(dir, 'main');

  expect(status.error).toBeUndefined();
  expect(status.branch).toBe('feature');
  expect(status.base).toBe('main');
  expect(status.ahead).toBe(1);
  expect(status.behind).toBe(0);
  expect(status.changedFiles).toContain('feature.txt');
  expect(status.dirty).toBe(false);
});

test('an uncommitted edit marks the workstream dirty', async () => {
  const dir = await initRepo();
  fs.writeFileSync(path.join(dir, 'README.md'), 'changed\n');

  const status = await workstreamGitStatus(dir, 'main');

  expect(status.error).toBeUndefined();
  expect(status.dirty).toBe(true);
  expect(status.uncommittedFiles).toBeGreaterThanOrEqual(1);
  // The edit isn't committed, so it doesn't show up as a committed diff vs base.
  expect(status.changedFiles).toEqual([]);
});

test('a non-git directory returns an error object without throwing', async () => {
  const dir = mkTmp('kild-wt-nogit-');

  const status = await workstreamGitStatus(dir);

  expect(status.error).toBeDefined();
  expect(status.path).toBe(dir);
  expect(status.branch).toBeNull();
  expect(status.ahead).toBe(0);
  expect(status.behind).toBe(0);
  expect(status.dirty).toBe(false);
  expect(status.uncommittedFiles).toBe(0);
  expect(status.changedFiles).toEqual([]);
});

test('a missing base ref is reported as an error, not a crash', async () => {
  const dir = await initRepo();

  const status = await workstreamGitStatus(dir, 'does-not-exist');

  expect(status.error).toBeDefined();
  expect(status.base).toBe('does-not-exist');
  expect(status.branch).toBe('main'); // branch still resolves
  expect(status.ahead).toBe(0);
  expect(status.behind).toBe(0);
  expect(status.changedFiles).toEqual([]);
});
