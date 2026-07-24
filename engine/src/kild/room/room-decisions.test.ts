import { expect, test } from 'bun:test';

import {
  applyDecisionMarkers,
  formatOpenDecisions,
  openDecisions,
  type RoomDecision,
} from './room-decisions.ts';

function post(from: string, text: string, ts = 1000) {
  return { from, text, ts, system: false };
}

test('needs-decision opens a keyed decision attributed to the poster', () => {
  const room: { decisions?: RoomDecision[] } = {};
  const changed = applyDecisionMarkers(
    room,
    post(
      'worker',
      'Blocked on a call.\nneeds-decision[api-shape]: REST or RPC for the new endpoint?',
    ),
  );
  expect(changed).toBe(true);
  expect(openDecisions(room)).toEqual([
    {
      key: 'api-shape',
      summary: 'REST or RPC for the new endpoint?',
      openedBy: 'worker',
      openedAt: 1000,
    },
  ]);
});

test('resolved closes only the named open decision and records the resolver', () => {
  const room: { decisions?: RoomDecision[] } = {};
  applyDecisionMarkers(room, post('worker', 'needs-decision[api-shape]: REST or RPC?'));
  applyDecisionMarkers(room, post('worker', 'needs-decision[auth]: token or session?'));
  const changed = applyDecisionMarkers(
    room,
    post('human', 'resolved[api-shape]: REST, matches the rest of the API', 2000),
  );
  expect(changed).toBe(true);
  expect(openDecisions(room).map((d) => d.key)).toEqual(['auth']);
  const resolved = room.decisions?.find((d) => d.key === 'api-shape');
  expect(resolved).toMatchObject({
    resolvedBy: 'human',
    resolvedAt: 2000,
    note: 'REST, matches the rest of the API',
  });
});

test('a later done-style post never closes an open decision (the invariant)', () => {
  const room: { decisions?: RoomDecision[] } = {};
  applyDecisionMarkers(room, post('worker', 'needs-decision[api-shape]: REST or RPC?'));
  const changed = applyDecisionMarkers(
    room,
    post('worker', 'Done — implemented, tests green, decision handled.'),
  );
  expect(changed).toBe(false);
  expect(openDecisions(room)).toHaveLength(1);
});

test('resolving an unknown key is a silent no-op', () => {
  const room: { decisions?: RoomDecision[] } = {};
  expect(applyDecisionMarkers(room, post('human', 'resolved[nothing-open]: n/a'))).toBe(false);
  expect(room.decisions ?? []).toHaveLength(0);
});

test('re-opening an open key refreshes its summary instead of duplicating', () => {
  const room: { decisions?: RoomDecision[] } = {};
  applyDecisionMarkers(room, post('worker', 'needs-decision[auth]: token or session?'));
  applyDecisionMarkers(
    room,
    post('worker', 'needs-decision[auth]: token, session, or mTLS?', 2000),
  );
  expect(room.decisions).toHaveLength(1);
  expect(openDecisions(room)[0]).toMatchObject({
    summary: 'token, session, or mTLS?',
    openedAt: 1000, // the original raise, not the refresh
  });
});

test('a resolved key can be raised again as a fresh decision', () => {
  const room: { decisions?: RoomDecision[] } = {};
  applyDecisionMarkers(room, post('worker', 'needs-decision[auth]: token or session?'));
  applyDecisionMarkers(room, post('human', 'resolved[auth]: token', 2000));
  applyDecisionMarkers(room, post('worker', 'needs-decision[auth]: rotate how often?', 3000));
  expect(room.decisions).toHaveLength(2);
  expect(openDecisions(room)).toEqual([
    { key: 'auth', summary: 'rotate how often?', openedBy: 'worker', openedAt: 3000 },
  ]);
});

test('resolved with no note leaves note unset', () => {
  const room: { decisions?: RoomDecision[] } = {};
  applyDecisionMarkers(room, post('worker', 'needs-decision[auth]: token or session?'));
  applyDecisionMarkers(room, post('human', 'resolved[auth]', 2000));
  expect(room.decisions?.[0]?.note).toBeUndefined();
  expect(openDecisions(room)).toHaveLength(0);
});

test('malformed markers are ignored: bad key charset, missing summary, mid-line mention', () => {
  const room: { decisions?: RoomDecision[] } = {};
  const text = [
    'needs-decision[bad key]: spaces in the key',
    'needs-decision[api-shape]:', // no summary
    'see the earlier needs-decision[api-shape]: marker for context', // not line-anchored
  ].join('\n');
  expect(applyDecisionMarkers(room, post('worker', text))).toBe(false);
  expect(room.decisions ?? []).toHaveLength(0);
});

test('system notices never touch the ledger', () => {
  const room: { decisions?: RoomDecision[] } = {};
  const changed = applyDecisionMarkers(room, {
    from: 'human',
    text: 'needs-decision[x]: from a notice',
    ts: 1000,
    system: true,
  });
  expect(changed).toBe(false);
  expect(room.decisions ?? []).toHaveLength(0);
});

test('formatOpenDecisions renders a compact attributed list', () => {
  const room: { decisions?: RoomDecision[] } = {};
  applyDecisionMarkers(room, post('worker', 'needs-decision[auth]: token or session?'));
  expect(formatOpenDecisions(room)).toBe('auth (token or session?, raised by @worker)');
  expect(formatOpenDecisions({})).toBe('');
});
