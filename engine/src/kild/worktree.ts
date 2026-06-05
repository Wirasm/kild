import { execFile as execFileCb } from 'node:child_process';
import path from 'node:path';
import { promisify } from 'node:util';

import type { SandboxFactory } from '@flue/runtime';
import { local } from '@flue/runtime/node';

import { kildHome } from './config.ts';

// execFile (no shell) + a branch-name allowlist: the brain's create_worktree tool
// feeds an LLM-generated branch name in here, so shell interpolation would be RCE.
const execFile = promisify(execFileCb);

/** A git worktree on disk — kild's local isolation strategy, the battery Flue
 *  lacks (its sandboxes are virtual / local-cwd / Daytona, never a worktree). */
export interface Worktree {
  branch: string;
  path: string;
}

function worktreesRoot(): string {
  return path.join(kildHome(), 'worktrees');
}

function assertSafeBranch(branch: string): void {
  if (branch.startsWith('-') || !/^[A-Za-z0-9._/-]+$/.test(branch)) {
    throw new Error(`invalid branch name: ${branch}`);
  }
}

/** Create an isolated git worktree on a fresh `kild/<branch>` branch. */
export async function createWorktree(repo: string, branch: string): Promise<Worktree> {
  assertSafeBranch(branch);
  const wtPath = path.join(worktreesRoot(), branch.replace(/\//g, '-'));
  const ref = `kild/${branch}`;
  await execFile('git', ['-C', repo, 'worktree', 'remove', '--force', wtPath]).catch(() => {});
  await execFile('git', ['-C', repo, 'worktree', 'add', '-B', ref, wtPath]);
  return { branch: ref, path: wtPath };
}

export async function listWorktrees(repo: string): Promise<Worktree[]> {
  const { stdout } = await execFile('git', ['-C', repo, 'worktree', 'list', '--porcelain']);
  const trees: Worktree[] = [];
  let cur: Partial<Worktree> = {};
  for (const line of stdout.split('\n')) {
    if (line.startsWith('worktree ')) cur = { path: line.slice('worktree '.length) };
    else if (line.startsWith('branch '))
      cur.branch = line.slice('branch '.length).replace('refs/heads/', '');
    else if (line === '' && cur.path) {
      trees.push({ path: cur.path, branch: cur.branch ?? '(detached)' });
      cur = {};
    }
  }
  return trees;
}

export async function removeWorktree(repo: string, wtPath: string): Promise<void> {
  await execFile('git', ['-C', repo, 'worktree', 'remove', '--force', wtPath]);
}

/** A Flue sandbox that runs the agent's shell/fs directly inside the worktree. */
export function worktreeSandbox(wt: Worktree): SandboxFactory {
  return local({ cwd: wt.path });
}
