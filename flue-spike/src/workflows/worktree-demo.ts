import fs from 'node:fs/promises';
import path from 'node:path';

import { createAgent, type FlueContext } from '@flue/runtime';

import { DEFAULT_MODEL } from '../kild/config.ts';
import { findProject } from '../kild/projects.ts';
import { createWorktree, listWorktrees, removeWorktree, worktreeSandbox } from '../kild/worktree.ts';

/**
 * BATTERY #1 — worktree-as-Sandbox.
 *
 * Proves a real git worktree slots into Flue's Sandbox seam: an agent bound to
 * `worktreeSandbox(wt)` runs its shell/fs directly in the worktree on disk. The
 * deterministic claim is verified with shell + node fs (no LLM needed); we also
 * fire one agent prompt to show an agent genuinely operating inside it.
 */
export async function run({ init, payload }: FlueContext) {
  const p = (payload ?? {}) as { project?: string; branch?: string; withAgent?: boolean };
  const project = await findProject(p.project ?? 'flue-spike');
  if (!project) throw new Error('register a project first: project=flue-spike');

  // 1. Isolated worktree on a new kild/ branch.
  const wt = await createWorktree(project.path, p.branch ?? 'spike-demo');

  // 2. An agent session whose sandbox IS the worktree.
  const agent = createAgent(() => ({ model: DEFAULT_MODEL, sandbox: worktreeSandbox(wt) }));
  const harness = await init(agent);
  const session = await harness.session();

  // Deterministic proof: shell write lands on disk inside the worktree.
  await session.shell('echo isolated > SPIKE_MARKER.txt');
  const onDisk = await fs.readFile(path.join(wt.path, 'SPIKE_MARKER.txt'), 'utf8');
  const branch = await session.shell('git branch --show-current');

  // Optional: prove an actual agent turn runs in the worktree (costs tokens).
  let agentSaw: string | undefined;
  if (p.withAgent) {
    const res = await session.prompt('Run `ls` and tell me whether SPIKE_MARKER.txt exists. One sentence.');
    agentSaw = res.text;
  }

  const treesWhileOpen = (await listWorktrees(project.path)).length;
  await removeWorktree(project.path, wt.path);

  return {
    worktree: wt,
    fileLandedInWorktree: onDisk.trim() === 'isolated',
    branchInWorktree: branch.stdout.trim(),
    isolatedBranch: branch.stdout.trim().startsWith('kild/'),
    worktreeCountWhileOpen: treesWhileOpen,
    agentSaw,
  };
}
