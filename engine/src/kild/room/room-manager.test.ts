import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { SessionCallbacks } from '../sessions.ts';
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
  const callbacks = new Map<string, SessionCallbacks | undefined>();
  const prompted: Array<{ id: string; text: string; from?: string }> = [];
  let spawnCount = 0;
  const ids = ['s-1', 's-2', 's-3', 'm-1', 'm-2', 'm-3'];
  let idIndex = 0;

  const manager = new RoomManager({
    registry: new RoomRegistry(),
    sessions: {
      subscribe: () => () => {},
      spawn: (id, req, _origin, sessionCallbacks) => {
        spawnCount += 1;
        if (options?.spawnThrowsAt === spawnCount) throw new Error(`spawn failed ${spawnCount}`);
        callbacks.set(id, sessionCallbacks);
        spawned.push({ id, agent: req.agent });
      },
      prompt: (id, text, from) => {
        prompted.push({ id, text, from });
        return true;
      },
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

  return { manager, spawned, stopped, callbacks, prompted };
}

async function openRoom(
  manager: RoomManager,
  participants: ParticipantSpec[],
  roomId: string = 'room-1',
  openedBy?: string,
) {
  return manager.open(roomId, { name: 'demo', cwd: '/tmp', participants, openedBy });
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

test('open transitions the room from opening to running', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'worker' }])).toMatchObject({ ok: true });
  const rooms = manager.liveRooms();
  expect(rooms).toHaveLength(1);
  expect(rooms[0]).toMatchObject({
    id: 'room-1',
    name: 'demo',
    state: 'running',
    participants: [{ name: 'worker' }],
    log: [],
  });
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

test('post to an unknown recipient returns rejected and is not recorded (no room spam)', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  // Addressing is structured: a typo'd handle is a clean error to the caller...
  expect(await manager.postFromHuman('room-1', 'hello', ['planner'])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'no such participant: @planner (in the room: @worker)',
  });
  // ...never recorded or turned into a room warning.
  expect(manager.messages('room-1')).toEqual([]);
});

test('an untargeted post defaults to the room lead', async () => {
  const { manager, prompted } = fixture();
  await openRoom(manager, [{ name: 'worker' }, { name: 'reviewer' }]);

  // No explicit `to` → delivered to the lead (worker = s-1), not dropped as "addressed nobody".
  expect(await manager.postAs('room-1', 'brain', 'gate approved')).toEqual({
    ok: true,
    value: { message: 'Posted to the room.' },
  });
  expect(prompted).toEqual([{ id: 's-1', text: '[#demo] @brain: gate approved', from: 'brain' }]);
  expect(manager.messages('room-1').map((message) => message.text)).toEqual(['gate approved']);
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

test('halt transitions the room to halted in live room snapshots', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(await manager.halt('room-1')).toMatchObject({ ok: true });
  const rooms = manager.liveRooms();
  expect(rooms).toHaveLength(1);
  expect(rooms[0]).toMatchObject({
    id: 'room-1',
    name: 'demo',
    state: 'halted',
    participants: [{ name: 'worker' }],
  });
  expect(rooms[0]?.log).toHaveLength(1);
  expect(rooms[0]?.log[0]).toMatchObject({
    roomId: 'room-1',
    from: 'human',
    to: [],
    text: 'Room halted by the operator.',
    system: true,
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

test('participant message_out returns invalid_state for halted rooms', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.halt('room-1');
  const result = await callbacks
    .get('s-1')
    ?.onMessage?.({ kind: 'message_out', text: '@human hi' });
  expect(result).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "room 'demo' is halted",
  });
});

test('participant invite returns invalid_state for halted rooms', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.halt('room-1');
  const result = await callbacks
    .get('s-1')
    ?.onInvite?.({ kind: 'invite', name: 'reviewer', agent: 'reviewer' });
  expect(result).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "room 'demo' is halted",
  });
});

test('participant close_room returns invalid_state for halted rooms', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.halt('room-1');
  const result = await callbacks.get('s-1')?.onCloseRoom?.({ kind: 'close_room' });
  expect(result).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "room 'demo' is halted",
  });
});

test('notifies a live non-participant opener about a participant post to @human without re-entering the room', async () => {
  const { manager, callbacks, prompted } = fixture();
  await openRoom(manager, [{ name: 'worker' }], 'room-1', 'brain-session');

  await callbacks
    .get('s-1')
    ?.onMessage?.({ kind: 'message_out', text: 'approve the gate?', to: ['human'] });

  expect(prompted).toEqual([
    {
      id: 'brain-session',
      from: 'kild',
      text: "[kild operator notification] Room 'demo': @worker posted to @human: approve the gate?",
    },
  ]);
  expect(manager.messages('room-1').map((message) => message.text)).toEqual(['approve the gate?']);
});

test('does not notify an opener that is a room participant', async () => {
  const { manager, callbacks, prompted } = fixture();
  await openRoom(manager, [{ name: 'worker' }], 'room-1', 's-1');

  await callbacks.get('s-1')?.onMessage?.({ kind: 'message_out', text: '@human approve?' });

  expect(prompted).toEqual([]);
});

test('notifies a live non-participant opener on halt and close with the final non-system post', async () => {
  const { manager, prompted } = fixture();
  await openRoom(manager, [{ name: 'worker' }], 'room-1', 'brain-session');
  await manager.postFromHuman('room-1', '@worker implementation committed');
  prompted.splice(0); // discard ordinary room delivery; assertions below are opener notifications only
  await manager.halt('room-1');
  await manager.close('room-1');

  expect(prompted).toEqual([
    {
      id: 'brain-session',
      from: 'kild',
      text: "[kild operator notification] Room 'demo' was halted. Final non-system post: @worker implementation committed",
    },
    {
      id: 'brain-session',
      from: 'kild',
      text: "[kild operator notification] Room 'demo' was closed and archived. Final non-system post: @worker implementation committed",
    },
  ]);
});

test('close transitions a halted room to archived closed state', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.halt('room-1');
  expect(await manager.close('room-1')).toEqual({
    ok: true,
    value: { message: "Room 'demo' closed." },
  });
  expect(manager.liveRooms()).toEqual([]);
  const archived = manager.archived();
  expect(archived).toHaveLength(1);
  expect(archived[0]).toMatchObject({
    id: 'room-1',
    name: 'demo',
    state: 'closed',
    participants: [{ name: 'worker' }],
  });
  expect(archived[0]?.log).toHaveLength(1);
  expect(archived[0]?.log[0]).toMatchObject({
    roomId: 'room-1',
    from: 'human',
    to: [],
    text: 'Room halted by the operator.',
    system: true,
  });
});
