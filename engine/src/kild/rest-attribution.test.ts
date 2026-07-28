import { expect, test } from 'bun:test';

import type { AttachIdentity } from './attach-token.ts';
import {
  resolveNewKildActor,
  resolveSendActor,
  resolveSpawnActor,
  resolveStopActor,
  UNATTRIBUTED,
} from './rest-attribution.ts';

/**
 * Attribution's two credentials, as the routes present them.
 *
 * `handleForSession` answers with a HANDLE (the kild manager's job — see
 * `kild-manager.test.ts` for the resolution itself); `identifyToken` places an attach
 * token. Here: which credential wins, and what happens when one is wrong.
 */
const HANDLE_BY_SESSION: Record<string, string> = {
  'coder-session': 'coder',
  // Same persona, different handle — the thing attribution used to be unable to say.
  'reviewer-session': 'reviewer',
};

const IDENTITY_BY_TOKEN: Record<string, AttachIdentity> = {
  'tok-honryo': { kildId: 'kild-1', handle: 'honryo' },
  'tok-claude': { kildId: 'kild-1', handle: 'claude' },
  'tok-elsewhere': { kildId: 'kild-2', handle: 'honryo' },
};

const deps = {
  handleForSession(sessionId: string) {
    const handle = HANDLE_BY_SESSION[sessionId];
    if (handle) return { ok: true as const, value: handle };
    return {
      ok: false as const,
      code: 'rejected' as const,
      message: `unknown session: ${sessionId}`,
    };
  },
  identifyToken(token: string): AttachIdentity | undefined {
    return IDENTITY_BY_TOKEN[token];
  },
};

test('a credential-less caller is labelled, not privileged', () => {
  // The label is a name on the message. Nothing in delivery treats it specially — it is
  // not a participant, not a default recipient, and not a rank.
  expect(resolveNewKildActor({}, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('kild new derives the kickoff actor from openedBy', () => {
  expect(resolveNewKildActor({ openedBy: 'coder-session' }, deps)).toEqual({
    ok: true,
    value: 'coder',
  });
});

test('a send with no credential resolves to the unattributed label — curl keeps working', () => {
  expect(resolveSendActor({ kildId: 'kild-1' }, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('send derives the actor from sessionId', () => {
  expect(resolveSendActor({ kildId: 'kild-1', sessionId: 'coder-session' }, deps)).toEqual({
    ok: true,
    value: 'coder',
  });
});

test('stop with no credential resolves to the unattributed label', () => {
  expect(resolveStopActor({ kildId: 'kild-1' }, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('stop derives the actor from sessionId', () => {
  expect(resolveStopActor({ kildId: 'kild-1', sessionId: 'reviewer-session' }, deps)).toEqual({
    ok: true,
    value: 'reviewer',
  });
});

test('mixed session identity and from rejects', () => {
  const input = { kildId: 'kild-1', sessionId: 'coder-session', from: 'coder' };
  expect(resolveSendActor(input, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  });
});

test('a credential-less legacy from also rejects', () => {
  expect(resolveNewKildActor({ from: 'coder' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  });
});

test('unknown session identity rejects rather than falling back to a label', () => {
  expect(resolveStopActor({ kildId: 'kild-1', sessionId: 'missing-session' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown session: missing-session',
  });
});

// ── The attach token: an attached agent naming itself ──────────────────────────
// Without it every attached sender was `human`, because an attached agent has no kild
// session by definition — so two harnesses in one kild could not be told apart.

test('an attach token is attributed to the handle it was minted for', () => {
  expect(resolveSendActor({ kildId: 'kild-1', token: 'tok-honryo' }, deps)).toEqual({
    ok: true,
    value: 'honryo',
  });
});

test('two attached agents in one kild are distinguishable — the whole point', () => {
  const honryo = resolveSendActor({ kildId: 'kild-1', token: 'tok-honryo' }, deps);
  const claude = resolveSendActor({ kildId: 'kild-1', token: 'tok-claude' }, deps);
  expect([honryo, claude]).toEqual([
    { ok: true, value: 'honryo' },
    { ok: true, value: 'claude' },
  ]);
});

test('a token from another kild is rejected, never silently downgraded', () => {
  // The handle is unique within ONE kild and means nothing outside it.
  expect(resolveSendActor({ kildId: 'kild-1', token: 'tok-elsewhere' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'attach token is not for kild kild-1',
  });
});

test('a bogus token is rejected, not treated as no credential at all', () => {
  expect(resolveSendActor({ kildId: 'kild-1', token: 'not-a-token' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown attach token',
  });
});

test('stop is attributed from a token too — one attribution path, not per-verb rules', () => {
  expect(resolveStopActor({ kildId: 'kild-1', token: 'tok-claude' }, deps)).toEqual({
    ok: true,
    value: 'claude',
  });
});

test('creating a kild takes a token: there is no kild yet to scope it to', () => {
  // The kickoff records who opened the kild, exactly as `openedBy` already does from
  // another kild's session.
  expect(resolveNewKildActor({ token: 'tok-elsewhere' }, deps)).toEqual({
    ok: true,
    value: 'honryo',
  });
});

test('a token still rejects on create when nothing minted it', () => {
  expect(resolveNewKildActor({ token: 'not-a-token' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown attach token',
  });
});

test('the token wins over a session id: it is scoped to the kild being addressed', () => {
  // A harness can be an owned agent of a parent kild AND attached to this one. For a
  // message into THIS kild, the handle it holds here is the true sender.
  expect(
    resolveSendActor({ kildId: 'kild-1', sessionId: 'coder-session', token: 'tok-claude' }, deps),
  ).toEqual({ ok: true, value: 'claude' });
});

// ── Spawning: the same derivation, because a `task` IS a send ──────────────────
// The spawner is written to the roster as `invitedBy` AND is the sender of the new agent's
// `task`. A caller-supplied `invitedBy` would therefore have been a forgery path for message
// senders, which is why the route stopped trusting it.

test('the spawner is derived from a session id, like every other actor', () => {
  expect(resolveSpawnActor({ kildId: 'kild-1', sessionId: 'coder-session' }, deps)).toEqual({
    ok: true,
    value: 'coder',
  });
});

test('an attached harness spawns as the handle its token names', () => {
  expect(resolveSpawnActor({ kildId: 'kild-1', token: 'tok-claude' }, deps)).toEqual({
    ok: true,
    value: 'claude',
  });
});

test('a credential-less spawn is labelled — the CLI and curl keep working', () => {
  expect(resolveSpawnActor({ kildId: 'kild-1' }, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('a caller-asserted invitedBy is rejected, and the error names that field', () => {
  expect(resolveSpawnActor({ kildId: 'kild-1', invitedBy: 'orchestrator' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'invitedBy is not allowed; the spawner is engine-derived',
  });
});

test('invitedBy is rejected on presence, not on type — there is nothing to validate', () => {
  // Any value a caller chose for itself is unacceptable, so a non-string is the same
  // rejection rather than a separate "must be a string" answer.
  for (const invitedBy of [42, null, { handle: 'orchestrator' }, ['a']]) {
    expect(resolveSpawnActor({ kildId: 'kild-1', invitedBy }, deps)).toMatchObject({
      ok: false,
      code: 'rejected',
    });
  }
});

test('an unknown spawn credential rejects rather than spawning as somebody', () => {
  expect(resolveSpawnActor({ kildId: 'kild-1', token: 'not-a-token' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown attach token',
  });
});

test('a token plus a caller-asserted from is still a rejection', () => {
  expect(resolveSendActor({ kildId: 'kild-1', token: 'tok-claude', from: 'claude' }, deps)).toEqual(
    {
      ok: false,
      code: 'rejected',
      message: 'from is not allowed; actor identity is engine-derived',
    },
  );
});
