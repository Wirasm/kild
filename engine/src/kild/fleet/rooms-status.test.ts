import { expect, test } from 'bun:test';

import { compactLiveRooms } from './rooms-status.ts';

test('a live room with no log compacts to an empty post list', () => {
  expect(
    compactLiveRooms([
      {
        id: 'room-1',
        name: 'ops',
        participants: [{ name: 'brain', agent: 'brain' }],
        log: [],
      },
    ]),
  ).toEqual([
    {
      id: 'room-1',
      name: 'ops',
      participants: [{ name: 'brain', agent: 'brain' }],
      posts: [],
    },
  ]);
});

test('a live room keeps only the last two posts and preserves order', () => {
  const rooms = [
    {
      id: 'room-1',
      name: 'ops',
      participants: [{ name: 'brain', agent: 'brain' }],
      log: [
        { id: 'm1', roomId: 'room-1', from: 'human', to: ['brain'], text: 'one', ts: 1 },
        { id: 'm2', roomId: 'room-1', from: 'brain', to: ['human'], text: 'two', ts: 2 },
        { id: 'm3', roomId: 'room-1', from: 'human', to: ['brain'], text: 'three', ts: 3 },
      ],
    },
  ];

  const compact = compactLiveRooms(rooms);
  expect(compact[0]?.posts.map((message) => message.id)).toEqual(['m2', 'm3']);
});

test('a live room with git status passes the git block through', () => {
  const git = {
    path: '/tmp/ws',
    branch: 'feature-x',
    base: 'main',
    ahead: 2,
    behind: 0,
    dirty: true,
    uncommittedFiles: 1,
    changedFiles: ['src/a.ts'],
    conflictsWithBase: null,
  };
  const compact = compactLiveRooms([
    { id: 'room-1', name: 'ops', participants: [{ name: 'brain', agent: 'brain' }], log: [], git },
  ]);
  expect(compact[0]?.git).toEqual(git);
});

test('a live room without git status has no git key', () => {
  const compact = compactLiveRooms([
    { id: 'room-1', name: 'ops', participants: [{ name: 'brain', agent: 'brain' }], log: [] },
  ]);
  expect(compact[0]).not.toHaveProperty('git');
});

test('compaction copies participant and post arrays without mutating the source room', () => {
  const rooms = [
    {
      id: 'room-1',
      name: 'ops',
      participants: [{ name: 'brain', agent: 'brain' }],
      log: [{ id: 'm1', roomId: 'room-1', from: 'human', to: ['brain'], text: 'one', ts: 1 }],
    },
  ];

  const compact = compactLiveRooms(rooms);
  expect(compact[0]?.participants).not.toBe(rooms[0]?.participants);
  expect(compact[0]?.posts).not.toBe(rooms[0]?.log);
  expect(rooms[0]?.log.map((message) => message.id)).toEqual(['m1']);
});
