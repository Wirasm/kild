import { afterAll, beforeAll, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { worktree } from './worktree-sandbox.ts';

const execFile = promisify(execFileCb);

// A throwaway git repo with one commit — `git worktree add` needs a committed HEAD.
let repo: string;
let root: string;

beforeAll(async () => {
  repo = mkdtempSync(path.join(tmpdir(), 'kild-wt-repo-'));
  root = mkdtempSync(path.join(tmpdir(), 'kild-wt-root-'));
  await execFile('git', ['-C', repo, 'init', '-q']);
  await execFile('git', ['-C', repo, 'config', 'user.email', 't@t']);
  await execFile('git', ['-C', repo, 'config', 'user.name', 't']);
  await execFile('git', ['-C', repo, 'commit', '-q', '--allow-empty', '-m', 'init']);
});

afterAll(() => {
  rmSync(repo, { recursive: true, force: true });
  rmSync(root, { recursive: true, force: true });
});

test('createSessionEnv materializes a worktree; cwd + writes land inside it', async () => {
  const { sandbox, cleanup } = worktree({ repo, branch: 'demo', root });
  const env = await sandbox.createSessionEnv({ id: 't' });
  const expected = path.join(root, 'demo');

  expect(env.cwd).toBe(expected);
  await env.writeFile('F.txt', 'hi');
  const onDisk = await Bun.file(path.join(expected, 'F.txt')).text();
  expect(onDisk).toBe('hi');

  // The worktree shows up in the repo's worktree list while open…
  const { stdout: before } = await execFile('git', ['-C', repo, 'worktree', 'list']);
  expect(before).toContain(expected);

  // …and cleanup() removes it (caller-managed; Flue has no teardown hook).
  await cleanup();
  const { stdout: after } = await execFile('git', ['-C', repo, 'worktree', 'list']);
  expect(after).not.toContain(expected);
});
