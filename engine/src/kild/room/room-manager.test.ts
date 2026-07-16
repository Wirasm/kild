import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { RoomManager } from './room-manager.ts';
import { RoomRegistry } from './room-registry.ts';
import { HUMAN, type ParticipantSpec } from './room-types.ts';

let tmp: string;
let prevHome: string | undefined;

beforeAll(() => {
  prevHome = process.env.KILD_HOME;
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-room-manager-'));
  process.env.KILD_HOME = tmp;
});

afterAll(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function fixture(options?: { agents?: string[]; spawnThrowsAt?: number; createId?: () => string }) {
  const spawned: Array<{ id: string; agent?: string }> = [];
  const stopped: string[] = [];
  let spawnCount = 0;
  const ids = ['s-1', 's-2', 's-3', 'm-1', 'm-2', 'm-3'];
  let idIndex = 0;

  const manager = new RoomManager({
    registry: new RoomRegistry(),
    sessions: {
      subscribe: () => () => {},
      spawn: (id, req) => {
        spawnCount += 1;
        if (options?.spawnThrowsAt === spawnCount) throw new Error(`spawn failed ${spawnCount}`);
        spawned.push({ id, agent: req.agent });
      },
      prompt: () => {},
      stop: (id) => stopped.push(id),
    },
    listAgents: async () =>
      (options?.agents ?? ['default', 'worker', 'reviewer', 'orchestrator']).map((name) => ({
        name,
        description: '',
        systemPrompt: '',
      })),
    createId: options?.createId ?? (() => ids[idIndex++] ?? `id-${idIndex}`),
  });

  return { manager, spawned, stopped };
}

async function openRoom(
  manager: RoomManager,
  participants: ParticipantSpec[],
  roomId: string = 'room-1',
) {
  return manager.open(roomId, { name: 'demo', cwd: '/tmp', participants });
}

test('rejects a duplicate room id and preserves the existing room', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'worker' }])).toMatchObject({ ok: true });
  expect(await openRoom(manager, [{ name: 'reviewer' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'duplicate room id: room-1',
  });
  expect(manager.liveRooms().map((room) => room.id)).toEqual(['room-1']);
});

test('rejects the reserved human participant handle', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: HUMAN }])).toEqual({
    ok: false,
    code: 'rejected',
    message: `participant name '${HUMAN}' is reserved`,
  });
});

test('rejects duplicate participant names within one open spec', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'worker' }, { name: 'worker' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'duplicate participant: @worker',
  });
});

test('rejects when room capacity would be exceeded', async () => {
  const { manager } = fixture();
  expect(
    await openRoom(
      manager,
      Array.from({ length: 9 }, (_value, index) => ({ name: `worker-${index}` })),
    ),
  ).toEqual({
    ok: false,
    code: 'rejected',
    message: 'room capacity exceeded (max 8 participants)',
  });
});

test('rejects an omitted-agent participant whose name is not a known agent', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown agent: planner',
  });
});

test('rejects an explicitly named unknown agent', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'worker', agent: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown agent: planner',
  });
});

test("accepts explicit agent:'default' as the generic escape hatch", async () => {
  const { manager, spawned } = fixture();
  const result = await openRoom(manager, [{ name: 'planner', agent: 'default' }]);
  expect(result).toMatchObject({ ok: true, value: { roomId: 'room-1' } });
  expect(spawned).toEqual([{ id: 's-1', agent: 'default' }]);
});

test('rolls back already spawned participants when a later spawn fails', async () => {
  const { manager, stopped } = fixture({ spawnThrowsAt: 2 });
  expect(await openRoom(manager, [{ name: 'worker' }, { name: 'reviewer' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'spawn failed 2',
  });
  expect(stopped).toEqual(['s-1']);
  expect(manager.liveRooms()).toEqual([]);
});

test('post returns not_found for an unknown room', async () => {
  const { manager } = fixture();
  expect(await manager.postFromHuman('missing', 'hello')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such room: missing',
  });
});

test('post to an unknown recipient returns rejected and keeps the existing warning behavior', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(await manager.postFromHuman('room-1', '@planner hello')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'no such participant: @planner (in the room: @worker)',
  });
  expect(manager.messages('room-1').map((message) => message.text)).toEqual([
    '@planner hello',
    'no such participant: @planner (in the room: @worker)',
  ]);
});

test('halt returns invalid_state when the room is already halted', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(await manager.halt('room-1')).toMatchObject({ ok: true });
  expect(await manager.halt('room-1')).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "room 'demo' is already halted",
  });
});

test('addParticipant returns invalid_state once a room is halted', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.halt('room-1');
  expect(await manager.addParticipant('room-1', { name: 'reviewer' })).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "room 'demo' is halted",
  });
});

test('close returns not_found for an unknown room', async () => {
  const { manager } = fixture();
  expect(await manager.close('missing')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such room: missing',
  });
});

test('post returns invalid_state for halted rooms', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.halt('room-1');
  expect(await manager.postAs('room-1', 'brain', 'still there?')).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "room 'demo' is halted",
  });
});
