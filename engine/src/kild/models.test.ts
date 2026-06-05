import { expect, test } from 'bun:test';

import type { ModelRegistry } from '@earendil-works/pi-coding-agent';

import { resolveModel, withRole } from './models.ts';

const registry = {
  find: (provider: string, id: string) =>
    provider === 'anthropic' && id === 'haiku' ? { provider, id } : undefined,
  getAll: () => [{ provider: 'anthropic', id: 'haiku' }],
} as unknown as ModelRegistry;

test('resolves provider/id', () => {
  expect(resolveModel(registry, 'anthropic/haiku')).toMatchObject({ id: 'haiku' });
});

test('resolves a bare id', () => {
  expect(resolveModel(registry, 'haiku')).toMatchObject({ provider: 'anthropic' });
});

test('throws on an unknown model (no silent fallback to pi default)', () => {
  expect(() => resolveModel(registry, 'anthropic/nope')).toThrow('unknown model');
});

test('no pattern resolves to undefined (use pi default)', () => {
  expect(resolveModel(registry, undefined)).toBeUndefined();
});

test('withRole wraps only when given instructions', () => {
  expect(withRole('hi', 'be terse')).toContain('<role>');
  expect(withRole('hi', null)).toBe('hi');
});
