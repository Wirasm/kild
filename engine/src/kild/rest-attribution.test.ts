import { expect, test } from 'bun:test';

import { resolveNewKildActor, resolveSendActor, resolveStopActor } from './rest-attribution.ts';

const deps = {
  resolveActor(sessionId: string) {
    if (sessionId === 'brain-session') return { ok: true as const, value: 'brain' };
    if (sessionId === 'anon-session') {
      return {
        ok: false as const,
        code: 'rejected' as const,
        message: "session 'anon-session' has no actor identity",
      };
    }
    return {
      ok: false as const,
      code: 'rejected' as const,
      message: `unknown session: ${sessionId}`,
    };
  },
};

test('sessionless kild new resolves to human', () => {
  expect(resolveNewKildActor({}, deps)).toEqual({
    ok: true,
    value: { actor: 'human', human: true },
  });
});

test('kild new derives kickoff actor from openedBy', () => {
  expect(resolveNewKildActor({ openedBy: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: { actor: 'brain', human: false },
  });
});

test('sessionless send resolves to human', () => {
  expect(resolveSendActor({}, deps)).toEqual({
    ok: true,
    value: { actor: 'human', human: true },
  });
});

test('send derives actor from sessionId', () => {
  expect(resolveSendActor({ sessionId: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: { actor: 'brain', human: false },
  });
});

test('sessionless stop resolves to human', () => {
  expect(resolveStopActor({}, deps)).toEqual({
    ok: true,
    value: { actor: 'human', human: true },
  });
});

test('stop derives actor from sessionId', () => {
  expect(resolveStopActor({ sessionId: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: { actor: 'brain', human: false },
  });
});

test('mixed session identity and from rejects', () => {
  expect(resolveSendActor({ sessionId: 'brain-session', from: 'brain' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  });
});

test('sessionless legacy from also rejects', () => {
  expect(resolveNewKildActor({ from: 'brain' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  });
});

test('unknown session identity rejects without human fallback', () => {
  expect(resolveStopActor({ sessionId: 'missing-session' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown session: missing-session',
  });
});

test('session-aware requests reject when the live session has no actor identity', () => {
  expect(resolveSendActor({ sessionId: 'anon-session' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: "session 'anon-session' has no actor identity",
  });
});
