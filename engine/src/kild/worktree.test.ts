import { expect, test } from 'bun:test';

import { assertSafeBranch, worktreePath, worktreeRef } from './worktree.ts';

test('assertSafeBranch accepts ordinary + slashed names', () => {
  expect(() => assertSafeBranch('fix-auth')).not.toThrow();
  expect(() => assertSafeBranch('feat/x')).not.toThrow();
  expect(() => assertSafeBranch('a.b_c-1')).not.toThrow();
});

test('assertSafeBranch rejects injection-shaped names', () => {
  expect(() => assertSafeBranch('--evil')).toThrow();
  expect(() => assertSafeBranch('$(rm -rf /)')).toThrow();
  expect(() => assertSafeBranch('a b')).toThrow();
  expect(() => assertSafeBranch('a;b')).toThrow();
});

test('worktreeRef prefixes kild/', () => {
  expect(worktreeRef('x')).toBe('kild/x');
  expect(worktreeRef('feat/x')).toBe('kild/feat/x');
});

test('worktreePath slashes become dashes under worktrees/', () => {
  expect(worktreePath('a/b').replace(/\\/g, '/')).toEndWith('/worktrees/a-b');
  expect(worktreePath('fix-auth').replace(/\\/g, '/')).toEndWith('/worktrees/fix-auth');
});

test('the derivation helpers reject before any I/O', () => {
  expect(() => worktreeRef('--x')).toThrow();
  expect(() => worktreePath('$(x)')).toThrow();
});
