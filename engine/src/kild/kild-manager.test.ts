import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { AgentCallbacks } from './agent-manager.ts';
import { DEFAULT_WAKE_CAP } from './inbox.ts';
import { formatDelivery, KildManager } from './kild-manager.ts';
import { KildRegistry } from './kild-registry.ts';
import type { AgentSpec, KildActionSuccess } from './kild-types.ts';
import { worktreePath } from './worktree.ts';

let tmp: string;
let prevHome: string | undefined;

beforeAll(() => {
  prevHome = process.env.KILD_HOME;
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-manager-'));
  process.env.KILD_HOME = tmp;
});

afterAll(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function fixture(options?: {
  personas?: string[];
  spawnThrowsAt?: number;
  createId?: () => string;
}) {
  const spawned: Array<{ id: string; persona?: string; forkFrom?: string }> = [];
  const stopped: string[] = [];
  /** Sessions that died on their own — a crash, not a `stop`, so nothing marked the roster.
   *  The case only `prompt()`'s return value can reveal. */
  const dead = new Set<string>();
  const callbacks = new Map<string, AgentCallbacks | undefined>();
  const prompted: Array<{ id: string; text: string; from?: string }> = [];
  let spawnCount = 0;
  const ids = ['s-1', 's-2', 's-3', 'm-1', 'm-2', 'm-3'];
  let idIndex = 0;
  let agentBus: ((msg: { agent: string; event: unknown }) => void) | undefined;

  const manager = new KildManager({
    registry: new KildRegistry(),
    sessions: {
      subscribe: (fn) => {
        agentBus = fn as typeof agentBus;
        return () => {};
      },
      spawn: (id, req, _origin, agentCallbacks) => {
        spawnCount += 1;
        if (options?.spawnThrowsAt === spawnCount) throw new Error(`spawn failed ${spawnCount}`);
        callbacks.set(id, agentCallbacks);
        spawned.push({ id, persona: req.persona, forkFrom: req.forkFrom });
      },
      prompt: (id, text, from) => {
        // The substrate answers whether the process was there, and a stopped session is
        // NOT. Returning `true` unconditionally is what made the silent-send-loss bug
        // invisible to this suite while it was reproducible on a live engine: the mock was
        // more forgiving than the thing it stands for (`AgentManager.prompt` returns false
        // for an id it has no session for, and `stop()` deletes the session).
        if (stopped.includes(id) || dead.has(id)) return false;
        prompted.push({ id, text, from });
        return true;
      },
      stop: (id) => stopped.push(id),
    },
    listPersonas: async () =>
      (options?.personas ?? ['default', 'coder', 'reviewer', 'orchestrator']).map((name) => ({
        name,
        description: '',
        systemPrompt: '',
      })),
    createId: options?.createId ?? (() => ids[idIndex++] ?? `id-${idIndex}`),
  });

  const emitAgent = (agent: string, event: unknown) => agentBus?.({ agent, event });
  const killSession = (id: string) => dead.add(id);
  return { manager, spawned, stopped, callbacks, prompted, emitAgent, killSession };
}

async function newKild(manager: KildManager, agents: AgentSpec[], kildId: string = 'kild-1') {
  return manager.create(kildId, { name: 'demo', cwd: tmp, agents });
}

/** Every send names its sender and its recipients — there is no other shape. */
const send = (manager: KildManager, to: string[], text: string, from = 'human') =>
  manager.send('kild-1', from, to, text);

test('rejects a duplicate kild id and preserves the existing kild', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'coder' }])).toMatchObject({ ok: true });
  expect(await newKild(manager, [{ handle: 'reviewer' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'duplicate kild id: kild-1',
  });
  expect(manager.liveKilds().map((kild) => kild.id)).toEqual(['kild-1']);
});

test('rejects duplicate agent handles within one create spec', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'coder' }, { handle: 'coder' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'duplicate agent: @coder',
  });
});

test('rejects when kild capacity would be exceeded', async () => {
  const { manager } = fixture();
  expect(
    await newKild(
      manager,
      Array.from({ length: 9 }, (_value, index) => ({ handle: `coder-${index}` })),
    ),
  ).toEqual({
    ok: false,
    code: 'rejected',
    message: 'kild capacity exceeded (max 8 agents)',
  });
});

// The rejection NAMES the personas that exist. It is not decoration: a persona is how a
// handle nobody holds becomes an agent, so this message is the discovery path — without the
// list, a caller's only recourse is another guess.
test('rejects an omitted-persona agent whose handle is not a known persona, naming what exists', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message:
      'unknown persona: planner (available: general, default, coder, reviewer, orchestrator)',
  });
});

test('rejects an explicitly named unknown persona', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'coder', persona: 'planner' }])).toMatchObject({
    ok: false,
    code: 'rejected',
    message: expect.stringContaining('unknown persona: planner (available: general,'),
  });
});

test("accepts explicit persona:'default' as the generic escape hatch", async () => {
  const { manager, spawned } = fixture();
  const result = await newKild(manager, [{ handle: 'planner', persona: 'default' }]);
  expect(result).toMatchObject({ ok: true, value: { kildId: 'kild-1' } });
  expect(spawned).toEqual([{ id: 's-1', persona: 'default' }]);
});

test('create registers the kild live with its roster — presence is the whole liveness signal', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'coder' }])).toMatchObject({ ok: true });
  const kilds = manager.liveKilds();
  expect(kilds).toHaveLength(1);
  expect(kilds[0]).toMatchObject({
    id: 'kild-1',
    name: 'demo',
    agents: [{ handle: 'coder' }],
    log: [],
  });
});

test('rolls back already spawned agents when a later spawn fails', async () => {
  const { manager, stopped } = fixture({ spawnThrowsAt: 2 });
  expect(await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'spawn failed 2',
  });
  expect(stopped).toEqual(['s-1']);
  expect(manager.liveKilds()).toEqual([]);
});

// ── Directed send: the sender names the recipients, always ───────────────────────────

test('send returns not_found for an unknown kild', async () => {
  const { manager } = fixture();
  expect(await manager.send('missing', 'human', ['coder'], 'hello')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such kild: missing',
  });
});

test('a send that names no recipient is REJECTED — never a default, never a broadcast', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  expect(await send(manager, [], 'who is this for?')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'a message must name at least one recipient',
  });
  // Not recorded, and nobody was woken "just in case".
  expect(manager.messages('kild-1')).toEqual([]);
  expect(prompted).toEqual([]);
});

test('a solo kild gets no free pass — an unaddressed send is still rejected', async () => {
  // The old 1:1 rule made "exactly one agent" mean "obviously that agent". Convenience
  // like that belongs in a client; the engine answers the same way at every size.
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await send(manager, [], 'fix the bug')).toMatchObject({ ok: false, code: 'rejected' });
  expect(prompted).toEqual([]);
});

test('send to an unknown recipient returns rejected naming the roster, and is not recorded', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  // A typo'd handle is a clean error to the caller...
  expect(await send(manager, ['planner'], 'hello')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'no such agent: @planner (in the kild: @coder)',
  });
  // ...never recorded or turned into a kild warning.
  expect(manager.messages('kild-1')).toEqual([]);
});

test('`human` is not a recipient the engine knows — it is an address like any other', async () => {
  // Nothing is reserved and nothing is virtual: to be reachable as @human you attach a
  // handle called `human`. Otherwise the roster says so.
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.send('kild-1', 'coder', ['human'], 'done')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'no such agent: @human (in the kild: @coder)',
  });
});

test('a named recipient is prompted, and only that one', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  expect(await manager.send('kild-1', 'brain', ['reviewer'], 'gate approved')).toEqual({
    ok: true,
    value: { message: 'Sent to the kild.', deliveredTo: ['reviewer'] },
  });
  expect(prompted).toEqual([{ id: 's-2', text: '[#demo] @brain: gate approved', from: 'brain' }]);
  expect(manager.messages('kild-1')?.map((message) => message.text)).toEqual(['gate approved']);
});

test('one message reaches every recipient it names', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  expect(await manager.send('kild-1', 'brain', ['coder', 'reviewer'], 'status?')).toMatchObject({
    ok: true,
    value: { deliveredTo: ['coder', 'reviewer'] },
  });
  expect(prompted.map((p) => p.id)).toEqual(['s-1', 's-2']);
  // Recorded once, addressed to both — one message, two deliveries.
  expect(manager.messages('kild-1')).toHaveLength(1);
  expect(manager.messages('kild-1')?.[0]?.to).toEqual(['coder', 'reviewer']);
});

test('a sender is never delivered its own message, even when it names itself', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  await manager.send('kild-1', 'coder', ['coder', 'reviewer'], 'thinking aloud');
  expect(prompted).toEqual([{ id: 's-2', text: '[#demo] @coder: thinking aloud', from: 'coder' }]);
});

test('an agent send with an empty `to` is rejected at the control line too', async () => {
  const { manager, callbacks } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  expect(await callbacks.get('s-1')?.onSend?.({ kind: 'send', to: [], text: 'anyone?' })).toEqual({
    ok: false,
    code: 'rejected',
    message: 'a message must name at least one recipient',
  });
});

test('formatDelivery frames the message with kild, sender, and text', () => {
  expect(formatDelivery('demo', 'orchestrator', 'do X')).toBe('[#demo] @orchestrator: do X');
});

test('spawning an agent records no message — a roster change is an event, not a message', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.spawnAgent('kild-1', { handle: 'reviewer' })).toMatchObject({ ok: true });
  expect(manager.messages('kild-1')).toEqual([]);
  expect(prompted).toEqual([]);
  expect(manager.liveKilds()[0]?.agents.map((a) => a.handle)).toEqual(['coder', 'reviewer']);
});

// ── task-on-spawn: the fused spawn+send ────────────────────────────────────────────────
// A spawn with no first message produces an agent that sits idle, so every real delegation
// was spawn-then-send. `task` fuses them exactly as `kickoff` does for create. It is NOT a
// stored field: the record of what an agent was asked to do is its first message, which a
// later revision can follow — a copy on the roster would go stale silently.

test('task is delivered as the new agent’s first message, from the spawner', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);

  const result = await manager.spawnAgent(
    'kild-1',
    { handle: 'reviewer' },
    { invitedBy: 'orchestrator', task: 'review the auth diff' },
  );
  expect(result).toMatchObject({ ok: true });

  const log = manager.messages('kild-1') ?? [];
  expect(log).toHaveLength(1);
  expect(log[0]).toMatchObject({
    seq: 1,
    from: 'orchestrator',
    to: ['reviewer'],
    text: 'review the auth diff',
  });
  // …and it actually reached the process, not just the log.
  expect(prompted.at(-1)).toMatchObject({
    id: 's-2',
    text: formatDelivery('demo', 'orchestrator', 'review the auth diff'),
  });
});

test('a spawn with no task delivers nothing — the agent starts idle', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  await manager.spawnAgent('kild-1', { handle: 'reviewer' }, { invitedBy: 'orchestrator' });
  expect(manager.messages('kild-1')).toEqual([]);
  expect(prompted).toEqual([]);
});

test('a task with no sender is refused before anything is spawned', async () => {
  const { manager, spawned } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  expect(await manager.spawnAgent('kild-1', { handle: 'reviewer' }, { task: 'do X' })).toEqual({
    ok: false,
    code: 'rejected',
    message: 'a task needs a sender: spawn from an identified caller',
  });
  expect(spawned).toHaveLength(1); // only the creator's own agent
  expect(manager.liveKilds()[0]?.agents.map((a) => a.handle)).toEqual(['orchestrator']);
});

// ── One verb: addressing an agent that isn't here brings it in ─────────────────────────
// An agent has no `spawn` tool. Spawning was never the goal — it produced an agent sitting
// idle — so `send` does both: a recipient the kild does not have is created, then delivered
// to. The REST path deliberately does NOT (see `createMissing`): a typo from a client must
// not cost a process.

test('an agent’s send creates a recipient the kild does not have, then delivers to it', async () => {
  const { manager, callbacks, spawned, prompted } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);

  const result = await callbacks.get('s-1')?.onSend?.({
    kind: 'send',
    to: ['reviewer'],
    text: 'review the auth diff',
  });
  expect(result).toMatchObject({ ok: true, value: { created: ['reviewer'] } });

  // Created…
  expect(spawned.map((s) => s.persona)).toEqual(['orchestrator', 'reviewer']);
  // …with the spawn edge recorded, taken from the session the line arrived on and never
  // from the line itself: the agent says who it is addressing, the engine says who asked.
  expect(manager.liveKilds()[0]?.agents.find((a) => a.handle === 'reviewer')).toMatchObject({
    invitedBy: 'orchestrator',
  });
  // …and delivered to, as one ordinary message on the log.
  expect(manager.messages('kild-1')?.[0]).toMatchObject({
    seq: 1,
    from: 'orchestrator',
    to: ['reviewer'],
    text: 'review the auth diff',
  });
  expect(prompted.at(-1)).toMatchObject({
    id: 's-2',
    text: formatDelivery('demo', 'orchestrator', 'review the auth diff'),
  });
});

test('persona and model describe who a created recipient is; the handle names the instance', async () => {
  const { manager, callbacks, spawned } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  await callbacks.get('s-1')?.onSend?.({
    kind: 'send',
    to: ['reviewer-auth'],
    text: 'the auth path only',
    persona: 'reviewer',
    model: 'openai-codex/gpt-5.6-sol',
  });
  expect(spawned.at(-1)?.persona).toBe('reviewer');
  expect(manager.liveIdentities()[0]?.agents.at(-1)).toMatchObject({
    handle: 'reviewer-auth',
    persona: 'reviewer',
    model: 'openai-codex/gpt-5.6-sol',
    invitedBy: 'orchestrator',
  });
});

test('a fan-out names several new handles at once — one persona, one brief', async () => {
  const { manager, callbacks } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  const result = await callbacks.get('s-1')?.onSend?.({
    kind: 'send',
    to: ['rev-1', 'rev-2', 'rev-3'],
    text: 'independently: is this diff safe to land?',
    persona: 'reviewer',
  });
  expect(result).toMatchObject({ ok: true, value: { created: ['rev-1', 'rev-2', 'rev-3'] } });
  expect(manager.liveIdentities()[0]?.agents.map((a) => a.handle)).toEqual([
    'orchestrator',
    'rev-1',
    'rev-2',
    'rev-3',
  ]);
  // One message, three recipients — not three sends.
  expect(manager.messages('kild-1')).toHaveLength(1);
});

test('a send naming an already-present agent creates nothing and reports nothing created', async () => {
  const { manager, callbacks, spawned } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }, { handle: 'coder' }]);
  const result = await callbacks.get('s-1')?.onSend?.({
    kind: 'send',
    to: ['coder'],
    text: 'ship it',
  });
  expect(result).toMatchObject({ ok: true });
  expect((result as { value: { created?: string[] } }).value.created).toBeUndefined();
  expect(spawned).toHaveLength(2); // the two from create, nothing new
});

test('an unresolvable persona rejects the whole send: nothing is created, nothing is sent', async () => {
  const { manager, callbacks, spawned } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  // Persona defaults to the handle, so this is also what a typo'd handle costs: a clean
  // rejection naming the personas that DO exist, rather than an agent nobody meant to make.
  const result = await callbacks.get('s-1')?.onSend?.({
    kind: 'send',
    to: ['reviewr'],
    text: 'review the auth diff',
  });
  expect(result).toMatchObject({ ok: false, code: 'rejected' });
  expect((result as { message: string }).message).toContain('reviewer');
  expect(spawned).toHaveLength(1);
  expect(manager.messages('kild-1')).toEqual([]);
});

test('one bad recipient among good ones creates NEITHER — the batch is validated first', async () => {
  const { manager, callbacks, spawned } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  const result = await callbacks.get('s-1')?.onSend?.({
    kind: 'send',
    to: ['coder', 'nobody-persona'],
    text: 'go',
  });
  expect(result).toMatchObject({ ok: false, code: 'rejected' });
  expect(spawned).toHaveLength(1);
  expect(manager.liveKilds()[0]?.agents.map((a) => a.handle)).toEqual(['orchestrator']);
});

test('the REST path still refuses an unknown recipient — a client typo costs no process', async () => {
  const { manager, spawned } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  // No `createMissing`: same call, opposite meaning. This is the one difference between an
  // agent inside the kild and a caller at the boundary.
  expect(await manager.send('kild-1', 'human', ['reviewer'], 'hi')).toMatchObject({
    ok: false,
    code: 'rejected',
  });
  expect(spawned).toHaveLength(1);
});

test('invitedBy rides the cheap roster view, so five identical personas are attributable', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }]);
  await manager.spawnAgent(
    'kild-1',
    { handle: 'reviewer-auth', persona: 'reviewer' },
    { invitedBy: 'orchestrator' },
  );
  await manager.spawnAgent('kild-1', { handle: 'reviewer-api', persona: 'reviewer' });

  const roster = manager.liveIdentities()[0]?.agents ?? [];
  expect(roster.find((a) => a.handle === 'reviewer-auth')?.invitedBy).toBe('orchestrator');
  // Absent, not invented: nobody in the kild invited this one.
  expect(roster.find((a) => a.handle === 'reviewer-api')?.invitedBy).toBeUndefined();
  expect(roster.find((a) => a.handle === 'orchestrator')?.invitedBy).toBeUndefined();
});

// ── A handle with nothing behind it ───────────────────────────────────────────────────
// A stopped agent stays on the roster (its handle never rebinds, its transcript stays
// addressable), so membership alone said "yes" to a send nothing could receive: it
// validated, the substrate's `false` was discarded, and the sender was told "Sent to the
// kild." while `idle` was flipped on a process that would never emit `agent_end` to flip it
// back. Two shapes, two fixes: `stop` is known in advance, a crash is only discoverable at
// the moment of delivery.

test('a send to a STOPPED agent is refused, and nothing is recorded', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  expect(manager.stopAgent('kild-1', 'coder')).toMatchObject({ ok: true });

  const result = await send(manager, ['coder'], 'more work');
  expect(result).toMatchObject({ ok: false, code: 'rejected' });
  // The refusal names who CAN still read something — the stopped agent is not an option.
  expect((result as { message: string }).message).toContain('@coder');
  expect((result as { message: string }).message).toContain('@reviewer');
  // Never recorded: a message nothing can receive must not read back as delivered.
  expect(manager.messages('kild-1')).toEqual([]);
  expect(prompted).toEqual([]);
});

test('a stopped agent does not have its idle state corrupted by a send', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  manager.stopAgent('kild-1', 'coder');
  const before = manager.liveIdentities()[0]?.agents.find((a) => a.handle === 'coder');
  expect(before).toMatchObject({ stopped: true, idle: true });

  await send(manager, ['coder'], 'more work');
  // Unchanged. It used to come back `idle: false` for a session that was already gone,
  // with nothing left that could ever set it back.
  expect(manager.liveIdentities()[0]?.agents.find((a) => a.handle === 'coder')).toMatchObject({
    stopped: true,
    idle: true,
  });
});

test('a mixed send is refused wholesale, like an unknown recipient already is', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  manager.stopAgent('kild-1', 'coder');
  expect(await send(manager, ['reviewer', 'coder'], 'both of you')).toMatchObject({
    ok: false,
    code: 'rejected',
  });
  // A partial send is the thing the sender would not be told about.
  expect(prompted).toEqual([]);
  expect(manager.messages('kild-1')).toEqual([]);
});

test('a session that DIED is discovered at delivery: the roster learns and the sender is told', async () => {
  const { manager, killSession } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  // A crash, not a stop — nothing marked the roster, so validation cannot know. Only the
  // substrate's answer reveals it.
  killSession('s-1');

  const result = await send(manager, ['coder'], 'are you there?');
  expect(result).toMatchObject({ ok: true });
  const value = (result as { value: KildActionSuccess }).value;
  expect(value.message).toContain('did not receive it');
  expect(value.deliveredTo).toEqual([]); // NOT "delivered to coder"
  // The roster now reflects reality, so the next send is refused up front.
  expect(manager.liveIdentities()[0]?.agents.find((a) => a.handle === 'coder')).toMatchObject({
    stopped: true,
    idle: true,
  });
  expect(await send(manager, ['coder'], 'again')).toMatchObject({ ok: false, code: 'rejected' });
});

test('one dead recipient does not swallow a live one', async () => {
  const { manager, killSession, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  killSession('s-1');
  const result = await send(manager, ['coder', 'reviewer'], 'status?');
  expect((result as { value: KildActionSuccess }).value.deliveredTo).toEqual(['reviewer']);
  expect(prompted.map((p) => p.id)).toEqual(['s-2']);
});

// ── The message cursor: seq, not ts ──────────────────────────────────────────────────

test('seq is strictly increasing within a kild — ts is not, and cannot be', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  for (const text of ['one', 'two', 'three', 'four']) await send(manager, ['coder'], text);

  const log = manager.messages('kild-1') ?? [];
  expect(log.map((m) => m.seq)).toEqual([1, 2, 3, 4]);
  // Not merely non-decreasing: a cursor that can repeat cannot page.
  for (let i = 1; i < log.length; i += 1) {
    expect((log[i]?.seq ?? 0) > (log[i - 1]?.seq ?? 0)).toBe(true);
  }
  // `ts` is Date.now() — several sends inside one millisecond tie, which is precisely why
  // it is not the cursor.
  expect(new Set(log.map((m) => m.ts)).size).toBeLessThanOrEqual(log.length);
});

test('the broadcast message carries the SAME seq the log recorded — replay is detectable', async () => {
  const { manager } = fixture();
  const broadcast: number[] = [];
  manager.subscribe((msg) => {
    if ('message' in msg) broadcast.push(msg.message.seq);
  });
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'one');
  await send(manager, ['coder'], 'two');
  expect(broadcast).toEqual([1, 2]);
  expect(manager.messages('kild-1')?.map((m) => m.seq)).toEqual(broadcast);
});

test('messages(since) is an exclusive cursor, and unknown kilds are undefined not empty', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  for (const text of ['one', 'two', 'three']) await send(manager, ['coder'], text);

  expect(manager.messages('kild-1')?.map((m) => m.text)).toEqual(['one', 'two', 'three']);
  expect(manager.messages('kild-1', 1)?.map((m) => m.text)).toEqual(['two', 'three']);
  expect(manager.messages('kild-1', 3)).toEqual([]);
  // "No such kild" and "a kild with nothing to say" are different answers.
  expect(manager.messages('never-existed')).toBeUndefined();
});

test('an archived kild still answers messages() — its log IS what it still is', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'one');
  await send(manager, ['coder'], 'two');
  await manager.stop('kild-1');

  expect(manager.messages('kild-1')?.map((m) => m.seq)).toEqual([1, 2]);
  expect(manager.messages('kild-1', 1)?.map((m) => m.text)).toEqual(['two']);
});

test('seq survives persistence: a reloaded registry reports the same cursors', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  for (const text of ['one', 'two', 'three']) await send(manager, ['coder'], text);
  await manager.stop('kild-1'); // written to $KILD_HOME/kilds/kild-1.json

  // A brand-new registry reads that file back — the cursor is a property of the log, not of
  // a counter that died with the process.
  const reloaded = new KildRegistry();
  const log = reloaded.log('kild-1');
  expect(log?.map((m) => m.text)).toEqual(['one', 'two', 'three']);
  expect(log?.map((m) => m.seq)).toEqual([1, 2, 3]);
});

// ── The cheap listing: identity and structure, nothing that costs ────────────────────

test('liveIdentities carries identity and roster only — no cost, no log, no git', async () => {
  const { manager, emitAgent } = fixture();
  await manager.create('kild-1', {
    name: 'demo',
    cwd: tmp,
    agents: [{ handle: 'coder' }],
    worktree: 'slice-x',
  });
  await manager.attach('kild-1', 'claude');
  emitAgent('s-1', { kind: 'stats', tokens: 900, cost: 0.33, context_pct: null });
  emitAgent('s-1', { kind: 'pi_session', id: 'aaaa', file: '/tmp/s.jsonl' });

  const [identity] = manager.liveIdentities();
  expect(identity).toEqual({
    id: 'kild-1',
    name: 'demo',
    cwd: tmp,
    worktree: 'slice-x',
    base: 'main',
    agents: [
      {
        handle: 'coder',
        ownership: 'owned',
        // The RESOLVED persona: spawned with no explicit one, so it runs its handle. The
        // roster used to store `undefined` while the process ran `coder`, so the wire and
        // the ledger each reported a different persona and neither the true one.
        persona: 'coder',
        model: undefined,
        idle: undefined,
        stopped: undefined,
      },
      {
        handle: 'claude',
        ownership: 'attached',
        persona: undefined,
        model: undefined,
        idle: true,
        stopped: undefined,
      },
    ],
  });
  // The costly halves are absent here and present on the status view.
  expect(identity).not.toHaveProperty('log');
  expect(identity).not.toHaveProperty('git');
  expect(identity).not.toHaveProperty('totals');
  expect(identity?.agents[0]).not.toHaveProperty('tokens');
  expect(identity?.agents[0]).not.toHaveProperty('piSessionFile');

  const [status] = await manager.liveKildsStatus();
  expect(status?.totals).toEqual({ tokens: 900, cost: 0.33 });
  expect(status?.agents[0]).toMatchObject({ tokens: 900, piSessionFile: '/tmp/s.jsonl' });
  expect(status).not.toHaveProperty('log');
});

test('liveKildStatus enriches ONE kild, so addressing one never probes the rest', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }], 'kild-1');
  await manager.create('kild-2', { name: 'other', cwd: tmp, agents: [{ handle: 'reviewer' }] });

  const one = await manager.liveKildStatus('kild-1');
  expect(one).toMatchObject({ id: 'kild-1', name: 'demo' });
  expect(one?.git?.path).toBe(tmp);
  expect(await manager.liveKildStatus('never-existed')).toBeUndefined();
});

// ── Stopping ONE agent (DELETE /api/kilds/:id/agents/:handle) ────────────────────────

test('stopAgent stops that agent only and keeps it on the roster, marked stopped', async () => {
  const { manager, stopped } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  expect(await manager.stopAgent('kild-1', 'coder')).toEqual({
    ok: true,
    value: { message: "Stopped @coder in kild 'demo'." },
  });
  expect(stopped).toEqual(['s-1']); // only the coder's session
  // It stays addressable: a handle is unique for the kild's lifetime and never rebinds,
  // so its transcript must remain reachable after the process is gone.
  const agents = manager.liveKilds()[0]?.agents ?? [];
  expect(agents.map((a) => a.handle)).toEqual(['coder', 'reviewer']);
  expect(agents.find((a) => a.handle === 'coder')).toMatchObject({ stopped: true, idle: true });
  expect(agents.find((a) => a.handle === 'reviewer')?.stopped).toBeUndefined();
});

test('stopAgent is idempotent, and refuses unknown handles and attached harnesses', async () => {
  const { manager, stopped } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.stopAgent('kild-1', 'coder');
  expect(await manager.stopAgent('kild-1', 'coder')).toEqual({
    ok: true,
    value: { message: '@coder was already stopped.' },
  });
  expect(stopped).toEqual(['s-1']); // not stopped twice
  expect(await manager.stopAgent('kild-1', 'ghost')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such agent: @ghost',
  });
  // An attached harness belongs to the human — kild never assumes it may end it.
  expect(await manager.stopAgent('kild-1', 'claude')).toEqual({
    ok: false,
    code: 'rejected',
    message: "@claude is attached — its harness is not kild's to stop",
  });
  expect(await manager.stopAgent('nope', 'coder')).toMatchObject({ code: 'not_found' });
});

// ── Recording a land, so the ledger can name the commit ──────────────────────────────

test('recordLand puts the merge sha on the kild and the ledger reports it', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'landproj-'));
  await manager.create('kild-1', {
    name: 'demo',
    cwd: project,
    agents: [{ handle: 'coder' }],
  });
  await send(manager, ['coder'], 'ship it');
  expect(manager.recordLand('kild-1', 'abcdef1234567890')).toEqual({
    ok: true,
    value: { message: "Kild 'demo' landed as abcdef1." },
  });
  expect(manager.recordLand('nope', 'abcdef1')).toMatchObject({ code: 'not_found' });
  await manager.stop('kild-1');
  const log = fs.readFileSync(path.join(project, '.kild', 'LOG.md'), 'utf8');
  // Inference could only ever say "contained in base"; a recorded sha names the commit.
  expect(log).toContain('merged into main as abcdef1');
});

// ── Stop: the one teardown verb ──────────────────────────────────────────────────────

test('stop returns not_found for an unknown kild', async () => {
  const { manager } = fixture();
  expect(await manager.stop('missing')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such kild: missing',
  });
});

test('stop archives a kild with history and takes it out of the live set', async () => {
  const { manager, stopped } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'ship it');
  expect(await manager.stop('kild-1')).toEqual({
    ok: true,
    value: { message: "Kild 'demo' stopped." },
  });
  expect(stopped).toEqual(['s-1']);
  expect(manager.liveKilds()).toEqual([]);
  const archived = manager.archived();
  expect(archived).toHaveLength(1);
  expect(archived[0]).toMatchObject({ id: 'kild-1', name: 'demo', agents: [{ handle: 'coder' }] });
  expect(archived[0]?.log.map((m) => m.text)).toEqual(['ship it']);
});

test('any agent may stop the kild — there is no rank that owns teardown', async () => {
  const { manager, callbacks, stopped } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);
  // s-2 was spawned second; under the old model only s-1 could have done this.
  expect(await callbacks.get('s-2')?.onStop?.({ kind: 'stop', reason: 'goal complete' })).toEqual({
    ok: true,
    value: { message: "Kild 'demo' stopped." },
  });
  expect(stopped).toEqual(['s-1', 's-2']);
  expect(manager.liveKilds()).toEqual([]);
});

// ── pi session identity: the terminal-resume handle ──────────────────────────────────

test('a pi_session event lands on the agent and rides the live view', async () => {
  const { manager, emitAgent } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'kick off'); // give the kild history to persist
  emitAgent('s-1', {
    kind: 'pi_session',
    id: 'aaaa-bbbb',
    file: '/home/u/.pi/agent/sessions/x/aaaa-bbbb.jsonl',
  });

  expect(manager.liveKilds()[0]?.agents[0]).toMatchObject({
    handle: 'coder',
    piSessionId: 'aaaa-bbbb',
    piSessionFile: '/home/u/.pi/agent/sessions/x/aaaa-bbbb.jsonl',
  });
});

test('pi session handles survive into the archived snapshot', async () => {
  const { manager, emitAgent } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'kick off');
  emitAgent('s-1', { kind: 'pi_session', id: 'aaaa-bbbb', file: '/tmp/s.jsonl' });
  await manager.stop('kild-1');

  expect(manager.archived()[0]?.agents[0]).toMatchObject({
    piSessionId: 'aaaa-bbbb',
    piSessionFile: '/tmp/s.jsonl',
  });
});

// ── attention state: idle rides the observer views ───────────────────────────────────

test('idle rides the live view — finished-and-waiting without parsing logs', async () => {
  const { manager, emitAgent } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);

  // coder finishes its turn; reviewer is still working.
  emitAgent('s-1', { kind: 'agent_end' });

  const [coder, reviewer] = manager.liveKilds()[0]?.agents ?? [];
  expect(coder).toMatchObject({ handle: 'coder', idle: true });
  expect(reviewer?.idle).toBeUndefined();

  // A delivered turn reactivates: idle clears in the view too.
  await send(manager, ['coder'], 'one more thing');
  expect(manager.liveKilds()[0]?.agents[0]).toMatchObject({
    handle: 'coder',
    idle: false,
  });
});

// ── cost rollup: stats events land on the agent and sum per kild ─────────────────────

test('stats events land on the agent and kilds carry a cost total', async () => {
  const { manager, emitAgent } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);

  emitAgent('s-1', { kind: 'stats', tokens: 1200, cost: 0.5, context_pct: 10 });
  emitAgent('s-1', { kind: 'stats', tokens: 3400, cost: 1.25, context_pct: 20 }); // latest wins
  emitAgent('s-2', { kind: 'stats', tokens: 600, cost: 0.25, context_pct: 5 });

  const [coder, reviewer] = manager.liveKilds()[0]?.agents ?? [];
  expect(coder).toMatchObject({ handle: 'coder', tokens: 3400, cost: 1.25 });
  expect(reviewer).toMatchObject({ handle: 'reviewer', tokens: 600, cost: 0.25 });

  const status = await manager.liveKildsStatus();
  expect(status[0]?.totals).toEqual({ tokens: 4000, cost: 1.5 });
});

test('a kild with no stats yet carries no totals (no zero-noise)', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  const status = await manager.liveKildsStatus();
  expect(status[0]?.totals).toBeUndefined();
});

test('agent costs survive into the archived snapshot', async () => {
  const { manager, emitAgent } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'kick off'); // history so close archives it
  emitAgent('s-1', { kind: 'stats', tokens: 900, cost: 0.33, context_pct: null });
  await manager.stop('kild-1');

  expect(manager.archived()[0]?.agents[0]).toMatchObject({ tokens: 900, cost: 0.33 });
});

// ── close lifecycle: the ledger entry, the close event, the declared hook ─────────────

test('stopping a kild with history appends its factual entry to .kild/LOG.md', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await send(manager, ['coder'], 'ship the fix');
  await manager.stop('kild-1');

  const log = fs.readFileSync(path.join(project, '.kild', 'LOG.md'), 'utf8');
  expect(log).toContain('demo (kild-1)');
  expect(log).toContain('- goal: ship the fix');
  // Facts only — never the tail of the message log.
  expect(log).not.toContain('outcome');
  expect(log).toContain('- landed: ');
  expect(log).toContain('- code: ');
});

test('stop emits a close event carrying the facts the engine holds', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  const closed: Array<Record<string, unknown>> = [];
  manager.subscribe((msg) => {
    if ('kildClosed' in msg) closed.push(msg.kildClosed as unknown as Record<string, unknown>);
  });
  await manager.create('kild-1', {
    name: 'demo',
    cwd: project,
    agents: [{ handle: 'coder' }],
    worktree: 'demo',
  });
  await send(manager, ['coder'], 'ship the fix');
  await manager.stop('kild-1');

  expect(closed).toHaveLength(1);
  expect(closed[0]).toMatchObject({
    kildId: 'kild-1',
    name: 'demo',
    cwd: project,
    worktree: 'demo',
    ledgerPath: path.join(project, '.kild', 'LOG.md'),
  });
  expect(String(closed[0]?.transcriptPath)).toContain('kild-1.json');
});

test('a declared hooks.onClose agent is spawned in the MAIN checkout with its facts', async () => {
  const { manager, spawned, prompted } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  fs.mkdirSync(path.join(project, '.kild'), { recursive: true });
  fs.writeFileSync(
    path.join(project, '.kild', 'config.json'),
    JSON.stringify({
      hooks: {
        onClose: {
          agent: {
            model: 'openai-codex/gpt-5.6-sol',
            persona: 'default',
            prompt: 'distill {{transcriptPath}} next to {{ledgerPath}}',
          },
        },
      },
    }),
  );
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await send(manager, ['coder'], 'ship the fix');
  const before = spawned.length;
  await manager.stop('kild-1');

  expect(spawned.length).toBe(before + 1);
  const hookPrompt = prompted.find((p) => p.text.startsWith('distill '));
  expect(hookPrompt?.from).toBe('kild');
  expect(hookPrompt?.text).toContain('kild-1.json');
  expect(hookPrompt?.text).toContain(path.join(project, '.kild', 'LOG.md'));
});

test('without a declared hooks.onClose, stop spawns nothing extra', async () => {
  const { manager, spawned } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await send(manager, ['coder'], 'ship the fix');
  const before = spawned.length;
  await manager.stop('kild-1');
  expect(spawned.length).toBe(before);
});

// ── kildDir (the review endpoints' kild→dir resolution) ──────────────────────

test('kildDir resolves a live kild without a worktree to its cwd', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  // base falls all the way through to 'main' (tmp is no git checkout, no config).
  // `repo` is the main checkout — where a land merges and disposal removes the tree from —
  // and equals `dir` for a kild that has no worktree of its own.
  expect(manager.kildDir('kild-1')).toEqual({
    ok: true,
    value: { name: 'demo', dir: tmp, repo: tmp, worktree: undefined, base: 'main' },
  });
});

test('kildDir resolves a worktree kild to the worktree path, with its base', async () => {
  const { manager } = fixture();
  await manager.create('kild-1', {
    name: 'demo',
    cwd: tmp,
    agents: [{ handle: 'coder' }],
    worktree: 'slice-x',
    base: 'develop',
  });
  const result = manager.kildDir('kild-1');
  expect(result.ok).toBe(true);
  if (result.ok) {
    expect(result.value.dir).toBe(worktreePath('slice-x'));
    expect(result.value.base).toBe('develop');
  }
});

test('kildDir on an unknown kild is not_found', () => {
  const { manager } = fixture();
  expect(manager.kildDir('nope')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such live kild: nope',
  });
});

test('kildDir on a closed (archived) kild is invalid_state', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await send(manager, ['coder'], 'hello'); // history → the close archives it
  await manager.stop('kild-1');
  expect(manager.kildDir('kild-1')).toEqual({
    ok: false,
    code: 'invalid_state',
    message: 'kild kild-1 is archived; its working dir is gone',
  });
});

test('archived snapshots keep cwd and base so history stays project-attributable', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'attrproj-'));
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await send(manager, ['coder'], 'work');
  await manager.stop('kild-1');
  expect(manager.archived()[0]).toMatchObject({ cwd: project });
});

// ── Attached agents ───────────────────────────────────────────────────────────
// kild registers these but never spawns them, so delivery inverts: it queues, and the
// harness pulls at its own turn boundary. Addressing is identical — an attached agent is
// named in `to` exactly like an owned one; only the transport differs.

test('attach registers an attached agent that is addressable but never spawned', async () => {
  const { manager, spawned } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.attach('kild-1', 'claude')).toMatchObject({
    ok: true,
    // …plus the attach token, which is what makes this handle attributable when it SENDS
    // (asserted below — an attached agent has no session to be recognised by).
    value: { message: "@claude attached to kild 'demo'." },
  });
  expect(spawned.map((s) => s.id)).toEqual(['s-1']); // the coder only
  expect(manager.liveKilds()[0]?.agents).toEqual([
    expect.objectContaining({ handle: 'coder' }),
    expect.objectContaining({ handle: 'claude', ownership: 'attached', idle: true }),
  ]);
});

test('the roster STATES the ownership for both kinds, and an attached agent carries no pi handles', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  const [coder, claude] = manager.liveKilds()[0]?.agents ?? [];
  // `ownership` is the field a client switches on, so it is always present. It used to be
  // omitted for owned agents on the reasoning that absent means owned — which made
  // `ownership === 'owned'` false for every owned agent.
  expect(coder).toMatchObject({ ownership: 'owned' });
  expect(claude).toMatchObject({ ownership: 'attached' });
  expect(claude?.piSessionFile).toBeUndefined();
});

test('attaching twice with the same handle is a no-op, not an error', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  expect(await manager.attach('kild-1', 'claude')).toMatchObject({
    ok: true,
    value: { message: "@claude is already attached to kild 'demo'." },
  });
  expect(manager.liveKilds()[0]?.agents).toHaveLength(2);
});

test('attach refuses to take over an owned agent handle', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.attach('kild-1', 'coder')).toEqual({
    ok: false,
    code: 'rejected',
    message: "@coder is already an owned agent in 'demo'",
  });
});

test('attach refuses a blank handle and an unknown kild', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.attach('kild-1', '  ')).toMatchObject({ ok: false, code: 'rejected' });
  expect(await manager.attach('nope', 'claude')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such kild: nope',
  });
});

test('a human-driven harness attaches under any handle it likes — nothing is reserved', async () => {
  // The human is not a participant the engine knows about; it becomes addressable the
  // same way every other external harness does.
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.attach('kild-1', 'human')).toMatchObject({ ok: true });
  await manager.send('kild-1', 'coder', ['human'], 'need a call on the schema');
  expect(manager.drain('kild-1', 'human')).toMatchObject({
    ok: true,
    value: { messages: [{ from: 'coder', text: 'need a call on the schema' }] },
  });
});

test('a message addressed to an attached agent is queued, not pushed', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  const before = prompted.length;
  await manager.send('kild-1', 'coder', ['claude'], 'review is done');
  expect(prompted).toHaveLength(before); // nothing to push to
  const drained = manager.drain('kild-1', 'claude');
  expect(drained).toMatchObject({
    ok: true,
    value: { idle: false, capped: false, messages: [{ from: 'coder', text: 'review is done' }] },
  });
});

test('drain is destructive and the empty drain is the idle signal', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.send('kild-1', 'coder', ['claude'], 'ping');

  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { idle: false } });
  const working = manager.liveKilds()[0]?.agents.find((p) => p.handle === 'claude');
  expect(working?.idle).toBe(false);

  // Second drain: nothing queued → empty, idle. No separate status verb exists, or needed.
  expect(manager.drain('kild-1', 'claude')).toEqual({
    ok: true,
    value: { messages: [], idle: true, capped: false },
  });
  const done = manager.liveKilds()[0]?.agents.find((p) => p.handle === 'claude');
  expect(done?.idle).toBe(true);
});

test('the wake cap stops a runaway sender from waking an attached agent forever', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  const wake = async () => {
    await manager.send('kild-1', 'coder', ['claude'], 'again');
    const result = manager.drain('kild-1', 'claude');
    return result.ok ? result.value : undefined;
  };
  for (let turn = 0; turn < DEFAULT_WAKE_CAP; turn += 1) {
    expect(await wake()).toMatchObject({ idle: false });
  }
  // The cap trips: the harness is told nothing (so it may stop), the mail is not eaten.
  expect(await wake()).toMatchObject({ messages: [], idle: true, capped: true });
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { capped: false } });
});

test('owned delivery is untouched by the attached branch', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }, { handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.send('kild-1', 'orchestrator', ['coder'], 'do X');
  expect(prompted.at(-1)).toEqual({
    id: 's-2',
    text: '[#demo] @orchestrator: do X',
    from: 'orchestrator',
  });
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { messages: [] } });
});

test('an inbox only ever sees messages that named it — it is not a firehose', async () => {
  // If unaddressed traffic reached inboxes the wake cap would trip on messages the
  // attached agent was never asked to read.
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }, { handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await send(manager, ['orchestrator'], 'kick off');
  await manager.send('kild-1', 'orchestrator', ['coder'], 'for you only');
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { messages: [] } });
});

test('one message splits across both transports: pushed to owned, queued to attached', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }, { handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.send('kild-1', 'orchestrator', ['coder', 'claude'], 'status?');
  expect(prompted.at(-1)).toEqual({
    id: 's-2',
    text: '[#demo] @orchestrator: status?',
    from: 'orchestrator',
  });
  expect(manager.drain('kild-1', 'claude')).toMatchObject({
    ok: true,
    value: { messages: [{ from: 'orchestrator', text: 'status?' }] },
  });
});

test('drain on an unknown kild or agent is not_found, and never on an owned one', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(manager.drain('nope', 'claude')).toMatchObject({ ok: false, code: 'not_found' });
  expect(manager.drain('kild-1', 'claude')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such agent: @claude',
  });
  expect(manager.drain('kild-1', 'coder')).toEqual({
    ok: false,
    code: 'rejected',
    message: '@coder is an owned agent — kild pushes to it',
  });
});

test('stopping a kild stops the sessions kild owns and never the attached harness', async () => {
  const { manager, stopped } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.send('kild-1', 'coder', ['claude'], 'unread'); // mail still queued
  expect(await manager.stop('kild-1')).toMatchObject({ ok: true });
  expect(stopped).toEqual(['s-1']);
  // The kild is gone, so the next drain is not_found — which the hook reads as silence.
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: false, code: 'not_found' });
});

test('an attached agent counts against kild capacity', async () => {
  const { manager } = fixture();
  await newKild(
    manager,
    Array.from({ length: 8 }, (_value, index) => ({
      handle: `coder-${index}`,
      persona: 'default',
    })),
  );
  expect(await manager.attach('kild-1', 'claude')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'kild capacity exceeded (max 8 agents)',
  });
});

test('an attached agent rides the archived snapshot with its ownership', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await send(manager, ['coder'], 'work');
  await manager.stop('kild-1');
  expect(manager.archived()[0]?.agents).toContainEqual(
    expect.objectContaining({ handle: 'claude', ownership: 'attached' }),
  );
});

// ── Forking a session onto a spawned agent ───────────────────────────────────────────────
// `forkFrom` seeds an agent from a COPY of an existing pi session file. The copy is the
// point: the source is never written, so two writers on one conversation are impossible and
// forking a LIVE agent's session is safe. A fork is a snapshot — it diverges at the moment
// it is taken and does not follow the original.
//
// The capability was never lost in the reshape (`agent.ts` still calls
// `PiSessionManager.forkFrom`, and the manager still passes `KILD_FORK_SESSION`), but the
// only REST route that reached it was deleted. This pins the domain half of the path so the
// route cannot be wired to a manager that silently drops the field.

test('forkFrom reaches the spawn request, so a forked agent is actually seeded', async () => {
  const { manager, spawned } = fixture();
  expect(await newKild(manager, [{ handle: 'coder' }])).toMatchObject({ ok: true });

  const result = await manager.spawnAgent('kild-1', {
    handle: 'forked',
    persona: 'coder', // a fork inherits history, not necessarily a same-named persona
    forkFrom: '/sessions/original.jsonl',
  });
  expect(result).toMatchObject({ ok: true });
  expect(spawned.at(-1)?.forkFrom).toBe('/sessions/original.jsonl');
});

test('an ordinary spawn carries no forkFrom, so a fresh agent starts fresh', async () => {
  const { manager, spawned } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.spawnAgent('kild-1', { handle: 'second', persona: 'reviewer' });
  expect(spawned.at(-1)?.forkFrom).toBeUndefined();
});

// ── Attribution ───────────────────────────────────────────────────────────────
// A message's `from` is the sender's HANDLE — the thing recipients address and the thing
// `to[]` names. Both credentials resolve here because a handle is kild-level knowledge:
// a kild session id (owned agents) and an attach token (harnesses kild does not own).

test('an owned agent is attributed to its HANDLE, never to the persona it runs', async () => {
  const { manager } = fixture();
  // The case that was attributed outright wrongly: handle `reviewer`, persona `coder`.
  await newKild(manager, [{ handle: 'reviewer', persona: 'coder' }]);
  expect(manager.handleForSession('s-1')).toEqual({ ok: true, value: 'reviewer' });
});

test('two agents sharing one persona are two senders on the log', async () => {
  const { manager } = fixture();
  await newKild(manager, [
    { handle: 'left', persona: 'coder' },
    { handle: 'right', persona: 'coder' },
  ]);
  for (const session of ['s-1', 's-2']) {
    const actor = manager.handleForSession(session);
    expect(actor).toMatchObject({ ok: true });
    if (actor.ok) await manager.send('kild-1', actor.value, ['left', 'right'], 'ping');
  }
  // Attributed by persona both lines read `coder`, and the log could not say who spoke.
  expect(manager.messages('kild-1')?.map((m) => m.from)).toEqual(['left', 'right']);
});

test('a session in no live kild has no handle to be attributed to', () => {
  const { manager } = fixture();
  // An agent kild spawned outside a kild is nobody's peer — there is no handle to invent.
  expect(manager.handleForSession('never-a-kild-agent')).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown session: never-a-kild-agent',
  });
});

test('attach mints a token that identifies exactly one (kild, handle)', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  const attached = await manager.attach('kild-1', 'claude');
  expect(attached).toMatchObject({ ok: true });
  if (!attached.ok) return;
  expect(manager.identifyToken(attached.value.token)).toEqual({
    kildId: 'kild-1',
    handle: 'claude',
  });
});

test('re-attaching returns the SAME token — a session-start hook revokes nothing', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  const first = await manager.attach('kild-1', 'claude');
  const again = await manager.attach('kild-1', 'claude');
  if (!first.ok || !again.ok) throw new Error('attach should have succeeded');
  expect(again.value.token).toBe(first.value.token);
  expect(manager.identifyToken(first.value.token)).toEqual({
    kildId: 'kild-1',
    handle: 'claude',
  });
});

test('two attached harnesses in one kild are two distinct senders, not two `human`s', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  const honryo = await manager.attach('kild-1', 'honryo');
  const claude = await manager.attach('kild-1', 'claude');
  if (!honryo.ok || !claude.ok) throw new Error('attach should have succeeded');
  expect(honryo.value.token).not.toBe(claude.value.token);

  for (const token of [honryo.value.token, claude.value.token]) {
    const identity = manager.identifyToken(token);
    expect(identity?.kildId).toBe('kild-1');
    await manager.send('kild-1', identity?.handle ?? 'human', ['coder'], 'hi');
  }
  expect(manager.messages('kild-1')?.map((m) => m.from)).toEqual(['honryo', 'claude']);
});

test("a kild's tokens die with the kild — nothing to expire, nothing on disk", async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  const attached = await manager.attach('kild-1', 'claude');
  expect(attached).toMatchObject({ ok: true });
  if (!attached.ok) return;
  await manager.stop('kild-1');
  expect(manager.identifyToken(attached.value.token)).toBeUndefined();
});
