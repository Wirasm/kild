import { expect, test } from 'bun:test';

import { AgentManager, agentEnv } from './agent-manager.ts';
import { worktreePath, worktreeRef } from './worktree.ts';

// The agent path derives AgentInfo.branch/worktreePath from the worktree name
// synchronously (no await, no subprocess). These assert that pure mapping — the same
// derivation AgentManager.spawn performs — independent of git.
test('prompt silently drops a dead or missing session', () => {
  const agents = new AgentManager();
  expect(agents.prompt('missing', 'kild stopped', 'kild')).toBe(false);
});

// The three `resolveActor` cases that stood here are gone with the method: attribution
// answers with a HANDLE, which is kild-level knowledge, so it is resolved by the kild
// manager (`handleForSession`) and covered by `kild-manager.test.ts`. The session substrate
// only ever knew the persona, which named the wrong thing.

test('agentEnv carries the fork source to the agent as KILD_FORK_SESSION', () => {
  const env = agentEnv(
    's-1',
    { cwd: '/proj', forkFrom: '/sessions/2026-07-24_abc.jsonl' },
    undefined,
  );
  expect(env.KILD_FORK_SESSION).toBe('/sessions/2026-07-24_abc.jsonl');
  expect(env.KILD_CWD).toBe('/proj');
  expect(env.KILD_SESSION_ID).toBe('s-1');
});

test('agentEnv leaves KILD_FORK_SESSION empty for an ordinary (fresh) spawn', () => {
  expect(agentEnv('s-2', { cwd: '/proj' }, undefined).KILD_FORK_SESSION).toBe('');
});

test('a worktree name maps to its kild/ branch and on-disk path', () => {
  const name = 'fix-auth';
  expect(worktreeRef(name)).toBe('kild/fix-auth');
  expect(worktreePath(name).replace(/\\/g, '/')).toEndWith('/worktrees/fix-auth');
});

test('a slashed worktree name keeps the slash in the ref, dashes the path', () => {
  expect(worktreeRef('feat/login')).toBe('kild/feat/login');
  expect(worktreePath('feat/login').replace(/\\/g, '/')).toEndWith('/worktrees/feat-login');
});

test('an unsafe worktree name throws before any I/O (spawn surfaces it as an error)', () => {
  expect(() => worktreeRef('--x')).toThrow();
  expect(() => worktreePath('a b')).toThrow();
});
