import { expect, test } from 'bun:test';

import { AttachTokens } from './attach-token.ts';

test('a token is opaque, and two handles never share one', () => {
  const tokens = new AttachTokens();
  const honryo = tokens.mint('kild-1', 'honryo');
  const claude = tokens.mint('kild-1', 'claude');
  expect(honryo).not.toBe(claude);
  // 32 random bytes, base64url: no kild id, no handle, nothing to parse or guess.
  expect(honryo).toMatch(/^[A-Za-z0-9_-]{43}$/);
  expect(honryo).not.toContain('kild-1');
  expect(honryo).not.toContain('honryo');
});

test('minting twice for one handle returns the SAME token — re-attach is idempotent', () => {
  const tokens = new AttachTokens();
  const first = tokens.mint('kild-1', 'claude');
  expect(tokens.mint('kild-1', 'claude')).toBe(first);
  // …and the identity it resolves to is unchanged, so a session-start hook calling attach
  // on every turn never invalidates the token the session is already using.
  expect(tokens.identify(first)).toEqual({ kildId: 'kild-1', handle: 'claude' });
});

test('the same handle in two kilds is two identities, because a handle is kild-scoped', () => {
  const tokens = new AttachTokens();
  const inOne = tokens.mint('kild-1', 'claude');
  const inTwo = tokens.mint('kild-2', 'claude');
  expect(inOne).not.toBe(inTwo);
  expect(tokens.identify(inOne)).toEqual({ kildId: 'kild-1', handle: 'claude' });
  expect(tokens.identify(inTwo)).toEqual({ kildId: 'kild-2', handle: 'claude' });
});

test('a token nothing minted is undefined — the caller decides that is a rejection', () => {
  const tokens = new AttachTokens();
  tokens.mint('kild-1', 'claude');
  expect(tokens.identify('made-up')).toBeUndefined();
  expect(tokens.identify('')).toBeUndefined();
});

test('forgetting a kild kills its tokens and only its tokens', () => {
  const tokens = new AttachTokens();
  const claude = tokens.mint('kild-1', 'claude');
  const honryo = tokens.mint('kild-1', 'honryo');
  const elsewhere = tokens.mint('kild-2', 'claude');

  tokens.forget('kild-1');

  expect(tokens.identify(claude)).toBeUndefined();
  expect(tokens.identify(honryo)).toBeUndefined();
  expect(tokens.identify(elsewhere)).toEqual({ kildId: 'kild-2', handle: 'claude' });
});

test('re-attaching after the kild is forgotten mints a fresh token', () => {
  const tokens = new AttachTokens();
  const first = tokens.mint('kild-1', 'claude');
  tokens.forget('kild-1');
  const second = tokens.mint('kild-1', 'claude');
  expect(second).not.toBe(first);
  expect(tokens.identify(first)).toBeUndefined();
});

test('forgetting an unknown kild is a no-op', () => {
  const tokens = new AttachTokens();
  const claude = tokens.mint('kild-1', 'claude');
  tokens.forget('never-existed');
  expect(tokens.identify(claude)).toEqual({ kildId: 'kild-1', handle: 'claude' });
});
