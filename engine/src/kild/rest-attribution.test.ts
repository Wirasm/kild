import { expect, test } from 'bun:test';

import {
  resolveNewKildActor,
  resolveSendActor,
  resolveStopActor,
  UNATTRIBUTED,
} from './rest-attribution.ts';

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

test('a sessionless caller is labelled, not privileged', () => {
  // The label is a name on the message. Nothing in delivery treats it specially — it is
  // not a participant, not a default recipient, and not a rank.
  expect(resolveNewKildActor({}, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('kild new derives kickoff actor from openedBy', () => {
  expect(resolveNewKildActor({ openedBy: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: 'brain',
  });
});

test('sessionless send resolves to the unattributed label', () => {
  expect(resolveSendActor({}, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('send derives actor from sessionId', () => {
  expect(resolveSendActor({ sessionId: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: 'brain',
  });
});

test('sessionless stop resolves to the unattributed label', () => {
  expect(resolveStopActor({}, deps)).toEqual({ ok: true, value: UNATTRIBUTED });
});

test('stop derives actor from sessionId', () => {
  expect(resolveStopActor({ sessionId: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: 'brain',
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

test('unknown session identity rejects rather than falling back to a label', () => {
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
