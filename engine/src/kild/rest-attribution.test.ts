import { expect, test } from 'bun:test';

import {
  resolveCloseRoomActor,
  resolveOpenRoomActor,
  resolvePostRoomActor,
} from './rest-room-attribution.ts';

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

test('sessionless room open resolves to human', () => {
  expect(resolveOpenRoomActor({}, deps)).toEqual({
    ok: true,
    value: { actor: 'human', human: true },
  });
});

test('room open derives kickoff actor from openedBy', () => {
  expect(resolveOpenRoomActor({ openedBy: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: { actor: 'brain', human: false },
  });
});

test('sessionless room post resolves to human', () => {
  expect(resolvePostRoomActor({}, deps)).toEqual({
    ok: true,
    value: { actor: 'human', human: true },
  });
});

test('room post derives actor from sessionId', () => {
  expect(resolvePostRoomActor({ sessionId: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: { actor: 'brain', human: false },
  });
});

test('sessionless room close resolves to human', () => {
  expect(resolveCloseRoomActor({}, deps)).toEqual({
    ok: true,
    value: { actor: 'human', human: true },
  });
});

test('room close derives actor from sessionId', () => {
  expect(resolveCloseRoomActor({ sessionId: 'brain-session' }, deps)).toEqual({
    ok: true,
    value: { actor: 'brain', human: false },
  });
});

test('mixed session identity and from rejects', () => {
  expect(resolvePostRoomActor({ sessionId: 'brain-session', from: 'brain' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  });
});

test('sessionless legacy from also rejects', () => {
  expect(resolveOpenRoomActor({ from: 'brain' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  });
});

test('unknown session identity rejects without human fallback', () => {
  expect(resolveCloseRoomActor({ sessionId: 'missing-session' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown session: missing-session',
  });
});

test('session-aware requests reject when the live session has no actor identity', () => {
  expect(resolvePostRoomActor({ sessionId: 'anon-session' }, deps)).toEqual({
    ok: false,
    code: 'rejected',
    message: "session 'anon-session' has no actor identity",
  });
});
