import { expect, test } from 'bun:test';

import { worktreePath, worktreeRef } from './worktree.ts';

// The session path derives SessionInfo.branch/worktreePath from the worktree name
// synchronously (no await, no subprocess). These assert that pure mapping — the same
// derivation SessionManager.spawn performs — independent of git.
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
