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

const isNudge = (p: { from?: string; text: string }) =>
  p.from === 'kild' && p.text.includes('finished your turn without posting');

// createId order: lead 'agent' session = s-1, invited worker session = s-2.
const WORKER = 's-2';

test('failsafe: a delegate that goes idle WITHOUT posting is nudged to report (once)', async () => {
  const { manager, callbacks, prompted, emitSession } = fixture({
    agents: ['default', 'agent', 'worker'],
  });
  // Lead (s-1) opens; lead invites a worker → worker.invitedBy = 'agent' (the lead).
  await openRoom(manager, [{ name: 'agent' }]);
  await callbacks.get('s-1')?.onInvite?.({ kind: 'invite', name: 'worker', agent: 'worker' });
  prompted.length = 0;

  // Worker finishes a turn having posted nothing → nudged (the delegate itself).
  emitSession(WORKER, { kind: 'agent_end' });
  const nudges = prompted.filter(isNudge);
  expect(nudges).toHaveLength(1);
  expect(nudges[0]).toMatchObject({ id: WORKER });
  expect(nudges[0]?.text).toContain('@agent'); // told to post to its inviter

  // A second agent_end without an intervening turn does NOT re-nudge (dedup).
  emitSession(WORKER, { kind: 'agent_end' });
  expect(prompted.filter(isNudge)).toHaveLength(1);
});

test('default: a delegate that POSTED before going idle is NOT nudged (its post is the signal)', async () => {
  const { manager, callbacks, prompted, emitSession } = fixture({
    agents: ['default', 'agent', 'worker'],
  });
  await openRoom(manager, [{ name: 'agent' }]);
  await callbacks.get('s-1')?.onInvite?.({ kind: 'invite', name: 'worker', agent: 'worker' });
  prompted.length = 0;

  // Worker reports via an explicit post_message, then its turn ends.
  await callbacks.get(WORKER)?.onMessage?.({ kind: 'message_out', text: 'done', to: ['agent'] });
  emitSession(WORKER, { kind: 'agent_end' });
  expect(prompted.filter(isNudge)).toHaveLength(0);
});

test('the human-invited lead going idle without posting IS nudged — toward @human (no one watches the roster)', async () => {
  const { manager, prompted, emitSession } = fixture({ agents: ['default', 'agent', 'worker'] });
  await openRoom(manager, [{ name: 'agent' }]);
  prompted.length = 0;
  emitSession('s-1', { kind: 'agent_end' });
  const nudges = prompted.filter(isNudge);
  expect(nudges).toHaveLength(1);
  expect(nudges[0]).toMatchObject({ id: 's-1' });
  expect(nudges[0]?.text).toContain('@human'); // the operator is an agent by default — signal it
});

test('a self-addressed post is not a report — the failsafe still nudges', async () => {
  const { manager, callbacks, prompted, emitSession } = fixture({
    agents: ['default', 'agent', 'worker'],
  });
  await openRoom(manager, [{ name: 'agent' }]);
  await callbacks.get('s-1')?.onInvite?.({ kind: 'invite', name: 'worker', agent: 'worker' });
  prompted.length = 0;

  // The #1141 misroute: the worker "reports" to itself. Delivered to no one → still blind.
  await callbacks.get(WORKER)?.onMessage?.({ kind: 'message_out', text: 'done', to: ['worker'] });
  emitSession(WORKER, { kind: 'agent_end' });
  expect(prompted.filter(isNudge)).toHaveLength(1);
});

test('a post to @human counts as a report — the human is a real recipient (the operator channel)', async () => {
  const { manager, callbacks, prompted, emitSession } = fixture({
    agents: ['default', 'agent', 'worker'],
  });
  await openRoom(manager, [{ name: 'agent' }]);
  prompted.length = 0;

  await callbacks
    .get('s-1')
    ?.onMessage?.({ kind: 'message_out', text: 'KILD_NOTIFY_OK', to: ['human'] });
  emitSession('s-1', { kind: 'agent_end' });
  expect(prompted.filter(isNudge)).toHaveLength(0);
});

test('a delivered turn re-arms the failsafe (idle-without-post next turn nudges again)', async () => {
  const { manager, callbacks, prompted, emitSession } = fixture({
    agents: ['default', 'agent', 'worker'],
  });
  await openRoom(manager, [{ name: 'agent' }]);
  await callbacks.get('s-1')?.onInvite?.({ kind: 'invite', name: 'worker', agent: 'worker' });

  emitSession(WORKER, { kind: 'agent_end' }); // idle without post → nudge 1
  await manager.postAs('room-1', 'agent', 'do more', ['worker']); // deliver a turn → re-arm
  emitSession(WORKER, { kind: 'agent_end' }); // idle without post again → nudge 2

  expect(prompted.filter(isNudge)).toHaveLength(2);
});

// ── keyed decisions: no decision leaves the system silently ──────────────────────────

test('a needs-decision post opens a keyed decision and blocks close until force', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await callbacks.get('s-1')?.onMessage?.({
    kind: 'message_out',
    text: 'Two options here.\nneeds-decision[api-shape]: REST or RPC?',
    to: ['human'],
  });

  const refused = await manager.close('room-1');
  expect(refused).toMatchObject({ ok: false, code: 'rejected' });
  expect((refused as { message: string }).message).toContain('api-shape');
  expect((refused as { message: string }).message).toContain('force');

  expect(await manager.close('room-1', { force: true })).toEqual({
    ok: true,
    value: { message: "Room 'demo' closed." },
  });
});

test('a resolved post closes the decision and unblocks an ordinary close', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await callbacks.get('s-1')?.onMessage?.({
    kind: 'message_out',
    text: 'needs-decision[api-shape]: REST or RPC?',
    to: ['human'],
  });
  await manager.postFromHuman('room-1', 'resolved[api-shape]: REST — matches the existing API');

  expect(await manager.close('room-1')).toMatchObject({ ok: true });
});

test('a later done post never masks an open decision (the fold invariant)', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await callbacks.get('s-1')?.onMessage?.({
    kind: 'message_out',
    text: 'needs-decision[auth]: token or session?',
    to: ['human'],
  });
  await callbacks.get('s-1')?.onMessage?.({
    kind: 'message_out',
    text: 'done: shipped it, all handled',
    to: ['human'],
  });
  expect(await manager.close('room-1')).toMatchObject({ ok: false, code: 'rejected' });
});

test('the lead cannot close (or force) past an open decision — operator only', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await callbacks.get('s-1')?.onMessage?.({
    kind: 'message_out',
    text: 'needs-decision[auth]: token or session?',
    to: ['human'],
  });
  const result = await callbacks.get('s-1')?.onCloseRoom?.({ kind: 'close_room' });
  expect(result).toMatchObject({ ok: false, code: 'rejected' });
  expect((result as { message: string }).message).toContain('operator');
  expect(manager.liveRooms()).toHaveLength(1); // the room survived
});

test('decisions ride the live view and the archived snapshot', async () => {
  const { manager, callbacks } = fixture();
  await openRoom(manager, [{ name: 'worker' }]);
  await callbacks.get('s-1')?.onMessage?.({
    kind: 'message_out',
    text: 'needs-decision[auth]: token or session?',
    to: ['human'],
  });

  expect(manager.liveRooms()[0]?.decisions).toMatchObject([
    { key: 'auth', summary: 'token or session?', openedBy: 'worker' },
  ]);

  await manager.close('room-1', { force: true });
  expect(manager.archived()[0]?.decisions).toMatchObject([{ key: 'auth', openedBy: 'worker' }]);
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

// ── attention state: idle/posted ride the observer views ─────────────────────────────

test('idle and posted ride the live view — finished-and-waiting without parsing logs', async () => {
  const { manager, callbacks, emitSession } = fixture();
  await openRoom(manager, [{ name: 'worker' }, { name: 'reviewer' }]);

  // worker finishes its turn having reported; reviewer is still working.
  await callbacks.get('s-1')?.onMessage?.({ kind: 'message_out', text: 'done', to: ['human'] });
  emitSession('s-1', { kind: 'agent_end' });

  const [worker, reviewer] = manager.liveRooms()[0]?.participants ?? [];
  expect(worker).toMatchObject({ name: 'worker', idle: true, posted: true });
  expect(reviewer?.idle).toBeUndefined();
  expect(reviewer?.posted).toBeUndefined();

  // A delivered turn reactivates: idle/posted reset in the view too.
  await manager.postFromHuman('room-1', 'one more thing', ['worker']);
  expect(manager.liveRooms()[0]?.participants[0]).toMatchObject({
    name: 'worker',
    idle: false,
    posted: false,
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
      memory: { synthesis: { model: 'openai-codex/gpt-5.6-sol', agent: 'default' } },
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
