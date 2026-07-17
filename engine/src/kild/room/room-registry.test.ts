import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { RoomRegistry } from './room-registry.ts';
import type { Room, RoomMessage } from './room-types.ts';

let tmp: string;
let prevHome: string | undefined;

beforeAll(() => {
  prevHome = process.env.KILD_HOME;
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-rooms-'));
  process.env.KILD_HOME = tmp; // RoomRegistry reads kildHome() in its constructor
});

afterAll(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function room(id: string, state: Room['state'] = 'running'): Room {
  return {
    id,
    name: 'demo',
    cwd: '/tmp',
    participants: [{ name: 'worker', sessionId: 's1', agent: 'worker' }],
    log: [],
    state,
  };
}

function msg(roomId: string, text: string): RoomMessage {
  return { id: `${roomId}-1`, roomId, from: 'human', to: ['worker'], text, ts: 1 };
}

test('appendMessage write-throughs the room log with lifecycle state; an empty room leaves no file', () => {
  const reg = new RoomRegistry();
  reg.create(room('room-a'));
  const file = path.join(tmp, 'rooms', 'room-a.json');
  expect(fs.existsSync(file)).toBe(false); // no messages yet → no history clutter
  reg.appendMessage('room-a', msg('room-a', 'hello'));
  expect(fs.existsSync(file)).toBe(true);
  expect(JSON.parse(fs.readFileSync(file, 'utf8'))).toMatchObject({
    id: 'room-a',
    state: 'running',
  });
});

test('a fresh registry loads past rooms into the archive (read-only) with their log', () => {
  const reg = new RoomRegistry(); // re-reads $KILD_HOME/rooms at construction
  const found = reg.archived().find((a) => a.id === 'room-a');
  expect(found).toBeDefined();
  expect(found?.log.map((m) => m.text)).toEqual(['hello']);
  expect(found?.participants).toEqual([{ name: 'worker', agent: 'worker' }]);
  expect(found?.state).toBe('running');
  // Archived rooms are history only — they are NOT live in-memory rooms.
  expect(reg.get('room-a')).toBeUndefined();
});

test('remove() archives a room with history immediately as closed and returns the snapshot', () => {
  const reg = new RoomRegistry();
  reg.create(room('room-b', 'closed'));
  reg.appendMessage('room-b', msg('room-b', 'hi'));
  const snap = reg.remove('room-b');
  expect(snap?.id).toBe('room-b');
  expect(snap?.state).toBe('closed');
  expect(reg.get('room-b')).toBeUndefined(); // no longer live
  expect(reg.archived().find((a) => a.id === 'room-b')?.state).toBe('closed'); // archived now, no restart
});

test('halted to closed archive state persists across reload', () => {
  const reg = new RoomRegistry();
  reg.create(room('room-d', 'halted'));
  reg.appendMessage('room-d', msg('room-d', 'halted first'));
  const live = reg.get('room-d');
  expect(live).toBeDefined();
  if (!live) throw new Error('expected room-d to be live before close');
  live.state = 'closed';
  expect(reg.remove('room-d')?.state).toBe('closed');

  const reloaded = new RoomRegistry();
  const found = reloaded.archived().find((room) => room.id === 'room-d');
  expect(found).toBeDefined();
  expect(found?.state).toBe('closed');
  expect(found?.log.map((message) => message.text)).toEqual(['halted first']);
});

test('remove() of an empty room archives nothing', () => {
  const reg = new RoomRegistry();
  reg.create(room('room-c'));
  expect(reg.remove('room-c')).toBeUndefined();
  expect(reg.archived().some((a) => a.id === 'room-c')).toBe(false);
});

test('appendMessage persists atomically: rename failure keeps the previous file and surfaces the error', () => {
  const reg = new RoomRegistry();
  reg.create(room('room-e'));
  reg.appendMessage('room-e', msg('room-e', 'first'));

  const file = path.join(tmp, 'rooms', 'room-e.json');
  const original = fs.readFileSync(file, 'utf8');
  const renameSync = fs.renameSync;
  fs.renameSync = ((from: fs.PathLike, to: fs.PathLike) => {
    if (String(to) === file) throw new Error('rename blocked');
    return renameSync(from, to);
  }) as typeof fs.renameSync;

  try {
    expect(() => reg.appendMessage('room-e', msg('room-e', 'second'))).toThrow(
      'kild: failed to persist room room-e: rename blocked',
    );
  } finally {
    fs.renameSync = renameSync;
  }

  expect(fs.readFileSync(file, 'utf8')).toBe(original);
  expect(
    fs
      .readdirSync(path.join(tmp, 'rooms'))
      .filter((name) => name.startsWith('room-e.json.') && name.endsWith('.tmp')),
  ).toEqual([]);
});

test('remove() surfaces archive persistence failures', () => {
  const reg = new RoomRegistry();
  reg.create(room('room-f', 'closed'));
  reg.appendMessage('room-f', msg('room-f', 'hi'));

  const renameSync = fs.renameSync;
  fs.renameSync = (() => {
    throw new Error('rename blocked');
  }) as typeof fs.renameSync;

  try {
    expect(() => reg.remove('room-f')).toThrow(
      'kild: failed to persist room room-f: rename blocked',
    );
  } finally {
    fs.renameSync = renameSync;
  }
});
