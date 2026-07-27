import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { SessionCallbacks } from '../sessions.ts';
import { worktreePath } from '../worktree.ts';
import { DEFAULT_WAKE_CAP } from './attached.ts';
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
  const spawned: Array<{ id: string; persona?: string }> = [];
  const stopped: string[] = [];
  const callbacks = new Map<string, SessionCallbacks | undefined>();
  const prompted: Array<{ id: string; text: string; from?: string }> = [];
  let spawnCount = 0;
  const ids = ['s-1', 's-2', 's-3', 'm-1', 'm-2', 'm-3'];
  let idIndex = 0;
  let sessionBus: ((msg: { session: string; event: unknown }) => void) | undefined;

  const manager = new RoomManager({
    registry: new RoomRegistry(),
    sessions: {
      subscribe: (fn) => {
        sessionBus = fn as typeof sessionBus;
        return () => {};
      },
      spawn: (id, req, _origin, sessionCallbacks) => {
        spawnCount += 1;
        if (options?.spawnThrowsAt === spawnCount) throw new Error(`spawn failed ${spawnCount}`);
        callbacks.set(id, sessionCallbacks);
        spawned.push({ id, persona: req.persona });
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

  const emitSession = (session: string, event: unknown) => sessionBus?.({ session, event });
  return { manager, spawned, stopped, callbacks, prompted, emitSession };
}

async function openRoom(
  manager: RoomManager,
  participants: ParticipantSpec[],
  roomId: string = 'room-1',
  openedBy?: string,
) {
  return manager.open(roomId, { name: 'demo', cwd: tmp, participants, openedBy });
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

test('rejects an omitted-persona participant whose name is not a known persona', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown persona: planner',
  });
});

test('rejects an explicitly named unknown persona', async () => {
  const { manager } = fixture();
  expect(await openRoom(manager, [{ name: 'worker', persona: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown persona: planner',
  });
});

test("accepts explicit persona:'default' as the generic escape hatch", async () => {
  const { manager, spawned } = fixture();
  const result = await openRoom(manager, [{ name: 'planner', persona: 'default' }]);
  expect(result).toMatchObject({ ok: true, value: { roomId: 'room-1' } });
  expect(spawned).toEqual([{ id: 's-1', persona: 'default' }]);
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
    value: { message: 'Posted to the room.', deliveredTo: ['worker'] },
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
    ?.onInvite?.({ kind: 'invite', name: 'reviewer', persona: 'reviewer' });
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

// ── pi session identity: the terminal-resume handle ──────────────────────────────────

test('a pi_session event lands on the participant and rides the live view', async () => {
  const { manager, emitSession } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.postFromHuman('room-1', 'kick off'); // give the room history to persist
  emitSession('s-1', {
    kind: 'pi_session',
    id: 'aaaa-bbbb',
    file: '/home/u/.pi/agent/sessions/x/aaaa-bbbb.jsonl',
  });

  expect(manager.liveRooms()[0]?.participants[0]).toMatchObject({
    name: 'worker',
    piSessionId: 'aaaa-bbbb',
    piSessionFile: '/home/u/.pi/agent/sessions/x/aaaa-bbbb.jsonl',
  });
});

test('pi session handles survive into the archived snapshot', async () => {
  const { manager, emitSession } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.postFromHuman('room-1', 'kick off');
  emitSession('s-1', { kind: 'pi_session', id: 'aaaa-bbbb', file: '/tmp/s.jsonl' });
  await manager.close('room-1');

  expect(manager.archived()[0]?.participants[0]).toMatchObject({
    piSessionId: 'aaaa-bbbb',
    piSessionFile: '/tmp/s.jsonl',
  });
});

// ── attention state: idle rides the observer views ───────────────────────────────────

test('idle rides the live view — finished-and-waiting without parsing logs', async () => {
  const { manager, emitSession } = fixture();
  await openRoom(manager, [{ name: 'worker' }, { name: 'reviewer' }]);

  // worker finishes its turn; reviewer is still working.
  emitSession('s-1', { kind: 'agent_end' });

  const [worker, reviewer] = manager.liveRooms()[0]?.participants ?? [];
  expect(worker).toMatchObject({ name: 'worker', idle: true });
  expect(reviewer?.idle).toBeUndefined();

  // A delivered turn reactivates: idle clears in the view too.
  await manager.postFromHuman('room-1', 'one more thing', ['worker']);
  expect(manager.liveRooms()[0]?.participants[0]).toMatchObject({
    name: 'worker',
    idle: false,
  });
});

// ── cost rollup: stats events land on the participant and sum per room ───────────────

test('stats events land on the participant and rooms carry a cost total', async () => {
  const { manager, emitSession } = fixture();
  await openRoom(manager, [{ name: 'worker' }, { name: 'reviewer' }]);

  emitSession('s-1', { kind: 'stats', tokens: 1200, cost: 0.5, context_pct: 10 });
  emitSession('s-1', { kind: 'stats', tokens: 3400, cost: 1.25, context_pct: 20 }); // latest wins
  emitSession('s-2', { kind: 'stats', tokens: 600, cost: 0.25, context_pct: 5 });

  const [worker, reviewer] = manager.liveRooms()[0]?.participants ?? [];
  expect(worker).toMatchObject({ name: 'worker', tokens: 3400, cost: 1.25 });
  expect(reviewer).toMatchObject({ name: 'reviewer', tokens: 600, cost: 0.25 });

  const status = await manager.liveRoomsStatus();
  expect(status[0]?.totals).toEqual({ tokens: 4000, cost: 1.5 });
});

test('a room with no stats yet carries no totals (no zero-noise)', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  const status = await manager.liveRoomsStatus();
  expect(status[0]?.totals).toBeUndefined();
});

test('participant costs survive into the archived snapshot', async () => {
  const { manager, emitSession } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.postFromHuman('room-1', 'kick off'); // history so close archives it
  emitSession('s-1', { kind: 'stats', tokens: 900, cost: 0.33, context_pct: null });
  await manager.close('room-1');

  expect(manager.archived()[0]?.participants[0]).toMatchObject({ tokens: 900, cost: 0.33 });
});

// ── memory hook: engine-written log on close, optional synthesis spawn ────────────────

test('closing a room with history appends its engine-written entry to .kild/LOG.md', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  await manager.open('room-1', { name: 'demo', cwd: project, participants: [{ name: 'worker' }] });
  await manager.postFromHuman('room-1', 'ship the fix');
  await manager.close('room-1');

  const log = fs.readFileSync(path.join(project, '.kild', 'LOG.md'), 'utf8');
  expect(log).toContain('demo (room-1)');
  expect(log).toContain('- goal: ship the fix');
});

test('memory.synthesis config spawns a synthesis session in the MAIN checkout after close', async () => {
  const { manager, spawned, prompted } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  fs.mkdirSync(path.join(project, '.kild'), { recursive: true });
  fs.writeFileSync(
    path.join(project, '.kild', 'config.json'),
    JSON.stringify({
      memory: { synthesis: { model: 'openai-codex/gpt-5.6-sol', persona: 'default' } },
    }),
  );
  await manager.open('room-1', { name: 'demo', cwd: project, participants: [{ name: 'worker' }] });
  await manager.postFromHuman('room-1', 'ship the fix');
  const before = spawned.length;
  await manager.close('room-1');

  expect(spawned.length).toBe(before + 1);
  const synthesisPromptDelivered = prompted.find((p) => p.text.includes('[kild memory synthesis]'));
  expect(synthesisPromptDelivered?.from).toBe('kild');
  expect(synthesisPromptDelivered?.text).toContain('.kild/MEMORY.md');
});

test('without memory.synthesis config, close spawns nothing extra', async () => {
  const { manager, spawned } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  await manager.open('room-1', { name: 'demo', cwd: project, participants: [{ name: 'worker' }] });
  await manager.postFromHuman('room-1', 'ship the fix');
  const before = spawned.length;
  await manager.close('room-1');
  expect(spawned.length).toBe(before);
});

// ── roomDir (the review endpoints' room→dir resolution) ───────────────────────

test('roomDir resolves a live room without a worktree to its cwd', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  // base falls all the way through to 'main' (tmp is no git checkout, no config).
  expect(manager.roomDir('room-1')).toEqual({
    ok: true,
    value: { dir: tmp, base: 'main' },
  });
});

test('roomDir resolves a worktree room to the worktree path, with its base', async () => {
  const { manager } = fixture();
  await manager.open('room-1', {
    name: 'demo',
    cwd: tmp,
    participants: [{ name: 'worker' }],
    worktree: 'slice-x',
    base: 'develop',
  });
  const result = manager.roomDir('room-1');
  expect(result.ok).toBe(true);
  if (result.ok) {
    expect(result.value.dir).toBe(worktreePath('slice-x'));
    expect(result.value.base).toBe('develop');
  }
});

test('roomDir on an unknown room is not_found', () => {
  const { manager } = fixture();
  expect(manager.roomDir('nope')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such live room: nope',
  });
});

test('roomDir on a closed (archived) room is invalid_state', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.postFromHuman('room-1', 'hello'); // history → the close archives it
  await manager.close('room-1');
  expect(manager.roomDir('room-1')).toEqual({
    ok: false,
    code: 'invalid_state',
    message: 'room room-1 is archived; its working dir is gone',
  });
});

test('archived snapshots keep cwd and base so history stays project-attributable', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'attrproj-'));
  await manager.open('room-1', { name: 'demo', cwd: project, participants: [{ name: 'worker' }] });
  await manager.postFromHuman('room-1', 'work');
  await manager.close('room-1');
  expect(manager.archived()[0]).toMatchObject({ cwd: project });
});

// ── Attached participants ─────────────────────────────────────────────────────
// kild registers these but never spawns them, so delivery inverts: it queues, and the
// harness pulls at its own turn boundary. Everything upstream — recipient resolution,
// the lead default, the ledger — is unchanged.

test('join registers an attached participant that is addressable but never spawned', async () => {
  const { manager, spawned } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(await manager.join('room-1', 'claude')).toEqual({
    ok: true,
    value: { message: "@claude attached to room 'demo'." },
  });
  expect(spawned.map((s) => s.id)).toEqual(['s-1']); // the worker only
  expect(manager.liveRooms()[0]?.participants).toEqual([
    expect.objectContaining({ name: 'worker' }),
    expect.objectContaining({ name: 'claude', kind: 'attached', idle: true }),
  ]);
});

test('the roster reports the kind, and an attached participant carries no pi handles', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  const [worker, claude] = manager.liveRooms()[0]?.participants ?? [];
  expect(worker?.kind).toBeUndefined(); // absent means spawned — no wire churn
  expect(claude).toMatchObject({ kind: 'attached' });
  expect(claude?.piSessionFile).toBeUndefined();
});

test('joining twice with the same name is a no-op, not an error', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  expect(await manager.join('room-1', 'claude')).toEqual({
    ok: true,
    value: { message: "@claude is already attached to room 'demo'." },
  });
  expect(manager.liveRooms()[0]?.participants).toHaveLength(2);
});

test('join refuses to take over a spawned participant handle', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(await manager.join('room-1', 'worker')).toEqual({
    ok: false,
    code: 'rejected',
    message: "@worker is already a spawned participant in 'demo'",
  });
});

test('join refuses the reserved human handle and an unknown room', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(await manager.join('room-1', HUMAN)).toMatchObject({ ok: false, code: 'rejected' });
  expect(await manager.join('nope', 'claude')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such room: nope',
  });
});

test('a post addressed to an attached participant is queued, not pushed', async () => {
  const { manager, prompted } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  const before = prompted.length;
  await manager.postAs('room-1', 'worker', 'review is done', ['claude']);
  expect(prompted).toHaveLength(before); // nothing to push to
  const drained = manager.drain('room-1', 'claude');
  expect(drained).toMatchObject({
    ok: true,
    value: { idle: false, capped: false, posts: [{ from: 'worker', text: 'review is done' }] },
  });
});

test('drain is destructive and the empty drain is the idle signal', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  await manager.postAs('room-1', 'worker', 'ping', ['claude']);

  expect(manager.drain('room-1', 'claude')).toMatchObject({ ok: true, value: { idle: false } });
  const working = manager.liveRooms()[0]?.participants.find((p) => p.name === 'claude');
  expect(working?.idle).toBe(false);

  // Second drain: nothing queued → empty, idle. No separate status verb exists, or needed.
  expect(manager.drain('room-1', 'claude')).toEqual({
    ok: true,
    value: { posts: [], idle: true, capped: false },
  });
  const done = manager.liveRooms()[0]?.participants.find((p) => p.name === 'claude');
  expect(done?.idle).toBe(true);
});

test('the wake cap stops a runaway sender from waking an attached participant forever', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  const wake = async () => {
    await manager.postAs('room-1', 'worker', 'again', ['claude']);
    const result = manager.drain('room-1', 'claude');
    return result.ok ? result.value : undefined;
  };
  for (let turn = 0; turn < DEFAULT_WAKE_CAP; turn += 1) {
    expect(await wake()).toMatchObject({ idle: false });
  }
  // The cap trips: the harness is told nothing (so it may stop), the mail is not eaten.
  expect(await wake()).toMatchObject({ posts: [], idle: true, capped: true });
  expect(manager.drain('room-1', 'claude')).toMatchObject({ ok: true, value: { capped: false } });
});

test('spawned delivery is untouched by the attached branch', async () => {
  const { manager, prompted } = fixture();
  await openRoom(manager, [{ name: 'orchestrator' }, { name: 'worker' }]);
  await manager.join('room-1', 'claude');
  await manager.postAs('room-1', 'orchestrator', 'do X', ['worker']);
  expect(prompted.at(-1)).toEqual({
    id: 's-2',
    text: '[#demo] @orchestrator: do X',
    from: 'orchestrator',
  });
  expect(manager.drain('room-1', 'claude')).toMatchObject({ ok: true, value: { posts: [] } });
});

test('a post with no addressee defaults to the lead and never fans out to mailboxes', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'orchestrator' }, { name: 'worker' }]);
  await manager.join('room-1', 'claude');
  await manager.postFromHuman('room-1', 'kick off');
  expect(manager.drain('room-1', 'claude')).toMatchObject({ ok: true, value: { posts: [] } });
});

test('drain on an unknown room or participant is not_found, and never on a spawned one', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  expect(manager.drain('nope', 'claude')).toMatchObject({ ok: false, code: 'not_found' });
  expect(manager.drain('room-1', 'claude')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such participant: @claude',
  });
  expect(manager.drain('room-1', 'worker')).toEqual({
    ok: false,
    code: 'rejected',
    message: '@worker is a spawned participant — kild pushes to it',
  });
});

test('closing a room stops the sessions kild owns and never the attached harness', async () => {
  const { manager, stopped } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  await manager.postAs('room-1', 'worker', 'unread', ['claude']); // mail still queued
  expect(await manager.close('room-1')).toMatchObject({ ok: true });
  expect(stopped).toEqual(['s-1']);
  // The room is gone, so the next drain is not_found — which the hook reads as silence.
  expect(manager.drain('room-1', 'claude')).toMatchObject({ ok: false, code: 'not_found' });
});

test('an attached participant counts against room capacity', async () => {
  const { manager } = fixture();
  await openRoom(
    manager,
    Array.from({ length: 8 }, (_value, index) => ({ name: `worker-${index}`, persona: 'default' })),
  );
  expect(await manager.join('room-1', 'claude')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'room capacity exceeded (max 8 participants)',
  });
});

test('an attached participant rides the archived snapshot with its kind', async () => {
  const { manager } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await manager.join('room-1', 'claude');
  await manager.postFromHuman('room-1', 'work');
  await manager.close('room-1');
  expect(manager.archived()[0]?.participants).toContainEqual(
    expect.objectContaining({ name: 'claude', kind: 'attached' }),
  );
});
