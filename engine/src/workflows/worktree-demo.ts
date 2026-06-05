import fs from 'node:fs/promises';
import path from 'node:path';

import { createAgent, type FlueContext } from '@flue/runtime';

import { worktree } from '../flue/worktree-sandbox.ts';
import { DEFAULT_MODEL } from '../kild/config.ts';
import { findProject } from '../kild/projects.ts';
import { listWorktrees, worktreesRoot } from '../kild/worktree.ts';

/**
 * BATTERY #1 — worktree-as-Sandbox.
 *
 * Proves a real git worktree slots into Flue's Sandbox seam: an agent bound to the
 * `worktree()` SandboxFactory runs its shell/fs directly in the worktree on disk.
 * The deterministic claim is verified with shell + node fs (no LLM needed); we also
 * fire one agent prompt to show an agent genuinely operating inside it. The
 * worktree() factory is the Flue-promotable mechanism (see flue/worktree-sandbox.ts).
 */
export async function run({ init, payload }: FlueContext) {
  const p = (payload ?? {}) as { project?: string; branch?: string; withAgent?: boolean };
  const project = await findProject(p.project ?? 'flue-spike');
  if (!project) throw new Error('register a project first: project=flue-spike');

  const branch = `kild/${p.branch ?? 'spike-demo'}`;
  const root = worktreesRoot();
  const wtPath = path.join(root, branch.replace(/\//g, '-'));

  // The Flue worktree() sandbox: createSessionEnv materializes the worktree, then
  // delegates the fs/exec env to local({cwd}) rooted inside it.
  const { sandbox, cleanup } = worktree({ repo: project.path, branch, root });
  const agent = createAgent(() => ({ model: DEFAULT_MODEL, sandbox }));
  const harness = await init(agent);
  const session = await harness.session();

  // Deterministic proof: shell write lands on disk inside the worktree.
  await session.shell('echo isolated > SPIKE_MARKER.txt');
  const onDisk = await fs.readFile(path.join(wtPath, 'SPIKE_MARKER.txt'), 'utf8');
  const branchShown = await session.shell('git branch --show-current');

  // Optional: prove an actual agent turn runs in the worktree (costs tokens).
  let agentSaw: string | undefined;
  if (p.withAgent) {
    const res = await session.prompt(
      'Run `ls` and tell me whether SPIKE_MARKER.txt exists. One sentence.',
    );
    agentSaw = res.text;
  }

  const treesWhileOpen = (await listWorktrees(project.path)).length;
  await cleanup();

  return {
    worktree: { branch, path: wtPath },
    fileLandedInWorktree: onDisk.trim() === 'isolated',
    branchInWorktree: branchShown.stdout.trim(),
    isolatedBranch: branchShown.stdout.trim().startsWith('kild/'),
    worktreeCountWhileOpen: treesWhileOpen,
    agentSaw,
  };
}
