import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { AgentCallbacks } from './agent-manager.ts';
import { DEFAULT_WAKE_CAP } from './inbox.ts';
import { KildManager } from './kild-manager.ts';
import { KildRegistry } from './kild-registry.ts';
import { type AgentSpec, HUMAN } from './kild-types.ts';
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
  const spawned: Array<{ id: string; persona?: string }> = [];
  const stopped: string[] = [];
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
        spawned.push({ id, persona: req.persona });
      },
      prompt: (id, text, from) => {
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
  return { manager, spawned, stopped, callbacks, prompted, emitAgent };
}

async function newKild(
  manager: KildManager,
  agents: AgentSpec[],
  kildId: string = 'kild-1',
  openedBy?: string,
) {
  return manager.create(kildId, { name: 'demo', cwd: tmp, agents, openedBy });
}

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

test('rejects the reserved human agent handle', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: HUMAN }])).toEqual({
    ok: false,
    code: 'rejected',
    message: `agent handle '${HUMAN}' is reserved`,
  });
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

test('rejects an omitted-persona agent whose handle is not a known persona', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown persona: planner',
  });
});

test('rejects an explicitly named unknown persona', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'coder', persona: 'planner' }])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'unknown persona: planner',
  });
});

test("accepts explicit persona:'default' as the generic escape hatch", async () => {
  const { manager, spawned } = fixture();
  const result = await newKild(manager, [{ handle: 'planner', persona: 'default' }]);
  expect(result).toMatchObject({ ok: true, value: { kildId: 'kild-1' } });
  expect(spawned).toEqual([{ id: 's-1', persona: 'default' }]);
});

test('create transitions the kild from opening to running', async () => {
  const { manager } = fixture();
  expect(await newKild(manager, [{ handle: 'coder' }])).toMatchObject({ ok: true });
  const kilds = manager.liveKilds();
  expect(kilds).toHaveLength(1);
  expect(kilds[0]).toMatchObject({
    id: 'kild-1',
    name: 'demo',
    state: 'running',
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

test('send returns not_found for an unknown kild', async () => {
  const { manager } = fixture();
  expect(await manager.sendFromHuman('missing', 'hello')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such kild: missing',
  });
});

test('send to an unknown recipient returns rejected and is not recorded (no kild spam)', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  // Addressing is structured: a typo'd handle is a clean error to the caller...
  expect(await manager.sendFromHuman('kild-1', 'hello', ['planner'])).toEqual({
    ok: false,
    code: 'rejected',
    message: 'no such agent: @planner (in the kild: @coder)',
  });
  // ...never recorded or turned into a kild warning.
  expect(manager.messageLog('kild-1')).toEqual([]);
});

test('an untargeted send defaults to the kild lead', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }, { handle: 'reviewer' }]);

  // No explicit `to` → delivered to the lead (coder = s-1), not dropped as "addressed nobody".
  expect(await manager.sendAs('kild-1', 'brain', 'gate approved')).toEqual({
    ok: true,
    value: { message: 'Sent to the kild.', deliveredTo: ['coder'] },
  });
  expect(prompted).toEqual([{ id: 's-1', text: '[#demo] @brain: gate approved', from: 'brain' }]);
  expect(manager.messageLog('kild-1').map((message) => message.text)).toEqual(['gate approved']);
});

test('halt returns invalid_state when the kild is already halted', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.halt('kild-1')).toMatchObject({ ok: true });
  expect(await manager.halt('kild-1')).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "kild 'demo' is already halted",
  });
});

test('spawnAgent returns invalid_state once a kild is halted', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.halt('kild-1');
  expect(await manager.spawnAgent('kild-1', { handle: 'reviewer' })).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "kild 'demo' is halted",
  });
});

test('halt transitions the kild to halted in live kild snapshots', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.halt('kild-1')).toMatchObject({ ok: true });
  const kilds = manager.liveKilds();
  expect(kilds).toHaveLength(1);
  expect(kilds[0]).toMatchObject({
    id: 'kild-1',
    name: 'demo',
    state: 'halted',
    agents: [{ handle: 'coder' }],
  });
  expect(kilds[0]?.log).toHaveLength(1);
  expect(kilds[0]?.log[0]).toMatchObject({
    kildId: 'kild-1',
    from: 'human',
    to: [],
    text: 'Kild halted by the operator.',
    system: true,
  });
});

test('stop returns not_found for an unknown kild', async () => {
  const { manager } = fixture();
  expect(await manager.stop('missing')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such kild: missing',
  });
});

test('send returns invalid_state for halted kilds', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.halt('kild-1');
  expect(await manager.sendAs('kild-1', 'brain', 'still there?')).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "kild 'demo' is halted",
  });
});

test('agent send returns invalid_state for halted kilds', async () => {
  const { manager, callbacks } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.halt('kild-1');
  const result = await callbacks.get('s-1')?.onSend?.({ kind: 'send', text: '@human hi' });
  expect(result).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "kild 'demo' is halted",
  });
});

test('agent spawn returns invalid_state for halted kilds', async () => {
  const { manager, callbacks } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.halt('kild-1');
  const result = await callbacks
    .get('s-1')
    ?.onSpawn?.({ kind: 'spawn', handle: 'reviewer', persona: 'reviewer' });
  expect(result).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "kild 'demo' is halted",
  });
});

test('agent stop returns invalid_state for halted kilds', async () => {
  const { manager, callbacks } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.halt('kild-1');
  const result = await callbacks.get('s-1')?.onStop?.({ kind: 'stop' });
  expect(result).toEqual({
    ok: false,
    code: 'invalid_state',
    message: "kild 'demo' is halted",
  });
});

test('notifies a live non-agent opener about an agent message to @human without re-entering the kild', async () => {
  const { manager, callbacks, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }], 'kild-1', 'brain-session');

  await callbacks.get('s-1')?.onSend?.({ kind: 'send', text: 'approve the gate?', to: ['human'] });

  expect(prompted).toEqual([
    {
      id: 'brain-session',
      from: 'kild',
      text: "[kild operator notification] Kild 'demo': @coder sent to @human: approve the gate?",
    },
  ]);
  expect(manager.messageLog('kild-1').map((message) => message.text)).toEqual([
    'approve the gate?',
  ]);
});

test('does not notify an opener that is a kild agent', async () => {
  const { manager, callbacks, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }], 'kild-1', 's-1');

  await callbacks.get('s-1')?.onSend?.({ kind: 'send', text: '@human approve?' });

  expect(prompted).toEqual([]);
});

test('notifies a live non-agent opener on halt and stop with the final non-system post', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }], 'kild-1', 'brain-session');
  await manager.sendFromHuman('kild-1', '@coder implementation committed');
  prompted.splice(0); // discard ordinary kild delivery; assertions below are opener notifications only
  await manager.halt('kild-1');
  await manager.stop('kild-1');

  expect(prompted).toEqual([
    {
      id: 'brain-session',
      from: 'kild',
      text: "[kild operator notification] Kild 'demo' was halted. Final non-system post: @coder implementation committed",
    },
    {
      id: 'brain-session',
      from: 'kild',
      text: "[kild operator notification] Kild 'demo' was stopped and archived. Final non-system post: @coder implementation committed",
    },
  ]);
});

test('stop transitions a halted kild to archived closed state', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.halt('kild-1');
  expect(await manager.stop('kild-1')).toEqual({
    ok: true,
    value: { message: "Kild 'demo' stopped." },
  });
  expect(manager.liveKilds()).toEqual([]);
  const archived = manager.archived();
  expect(archived).toHaveLength(1);
  expect(archived[0]).toMatchObject({
    id: 'kild-1',
    name: 'demo',
    state: 'closed',
    agents: [{ handle: 'coder' }],
  });
  expect(archived[0]?.log).toHaveLength(1);
  expect(archived[0]?.log[0]).toMatchObject({
    kildId: 'kild-1',
    from: 'human',
    to: [],
    text: 'Kild halted by the operator.',
    system: true,
  });
});

// ── pi session identity: the terminal-resume handle ──────────────────────────────────

test('a pi_session event lands on the agent and rides the live view', async () => {
  const { manager, emitAgent } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.sendFromHuman('kild-1', 'kick off'); // give the kild history to persist
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
  await manager.sendFromHuman('kild-1', 'kick off');
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
  await manager.sendFromHuman('kild-1', 'one more thing', ['coder']);
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
  await manager.sendFromHuman('kild-1', 'kick off'); // history so close archives it
  emitAgent('s-1', { kind: 'stats', tokens: 900, cost: 0.33, context_pct: null });
  await manager.stop('kild-1');

  expect(manager.archived()[0]?.agents[0]).toMatchObject({ tokens: 900, cost: 0.33 });
});

// ── memory hook: engine-written log on close, optional synthesis spawn ────────────────

test('stopping a kild with history appends its engine-written entry to .kild/LOG.md', async () => {
  const { manager } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await manager.sendFromHuman('kild-1', 'ship the fix');
  await manager.stop('kild-1');

  const log = fs.readFileSync(path.join(project, '.kild', 'LOG.md'), 'utf8');
  expect(log).toContain('demo (kild-1)');
  expect(log).toContain('- goal: ship the fix');
});

test('memory.synthesis config spawns a synthesis session in the MAIN checkout after stop', async () => {
  const { manager, spawned, prompted } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  fs.mkdirSync(path.join(project, '.kild'), { recursive: true });
  fs.writeFileSync(
    path.join(project, '.kild', 'config.json'),
    JSON.stringify({
      memory: { synthesis: { model: 'openai-codex/gpt-5.6-sol', persona: 'default' } },
    }),
  );
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await manager.sendFromHuman('kild-1', 'ship the fix');
  const before = spawned.length;
  await manager.stop('kild-1');

  expect(spawned.length).toBe(before + 1);
  const synthesisPromptDelivered = prompted.find((p) => p.text.includes('[kild memory synthesis]'));
  expect(synthesisPromptDelivered?.from).toBe('kild');
  expect(synthesisPromptDelivered?.text).toContain('.kild/MEMORY.md');
});

test('without memory.synthesis config, stop spawns nothing extra', async () => {
  const { manager, spawned } = fixture();
  const project = fs.mkdtempSync(path.join(tmp, 'memproj-'));
  await manager.create('kild-1', { name: 'demo', cwd: project, agents: [{ handle: 'coder' }] });
  await manager.sendFromHuman('kild-1', 'ship the fix');
  const before = spawned.length;
  await manager.stop('kild-1');
  expect(spawned.length).toBe(before);
});

// ── kildDir (the review endpoints' kild→dir resolution) ──────────────────────

test('kildDir resolves a live kild without a worktree to its cwd', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  // base falls all the way through to 'main' (tmp is no git checkout, no config).
  expect(manager.kildDir('kild-1')).toEqual({
    ok: true,
    value: { dir: tmp, base: 'main' },
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
  await manager.sendFromHuman('kild-1', 'hello'); // history → the close archives it
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
  await manager.sendFromHuman('kild-1', 'work');
  await manager.stop('kild-1');
  expect(manager.archived()[0]).toMatchObject({ cwd: project });
});

// ── Attached agents ───────────────────────────────────────────────────────────
// kild registers these but never spawns them, so delivery inverts: it queues, and the
// harness pulls at its own turn boundary. Everything upstream — recipient resolution,
// the lead default, the ledger — is unchanged.

test('attach registers an attached agent that is addressable but never spawned', async () => {
  const { manager, spawned } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.attach('kild-1', 'claude')).toEqual({
    ok: true,
    value: { message: "@claude attached to kild 'demo'." },
  });
  expect(spawned.map((s) => s.id)).toEqual(['s-1']); // the coder only
  expect(manager.liveKilds()[0]?.agents).toEqual([
    expect.objectContaining({ handle: 'coder' }),
    expect.objectContaining({ handle: 'claude', ownership: 'attached', idle: true }),
  ]);
});

test('the roster reports the ownership, and an attached agent carries no pi handles', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  const [coder, claude] = manager.liveKilds()[0]?.agents ?? [];
  expect(coder?.ownership).toBeUndefined(); // absent means owned — no wire churn
  expect(claude).toMatchObject({ ownership: 'attached' });
  expect(claude?.piSessionFile).toBeUndefined();
});

test('attaching twice with the same handle is a no-op, not an error', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  expect(await manager.attach('kild-1', 'claude')).toEqual({
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

test('attach refuses the reserved human handle and an unknown kild', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  expect(await manager.attach('kild-1', HUMAN)).toMatchObject({ ok: false, code: 'rejected' });
  expect(await manager.attach('nope', 'claude')).toEqual({
    ok: false,
    code: 'not_found',
    message: 'no such kild: nope',
  });
});

test('a message addressed to an attached agent is queued, not pushed', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  const before = prompted.length;
  await manager.sendAs('kild-1', 'coder', 'review is done', ['claude']);
  expect(prompted).toHaveLength(before); // nothing to push to
  const drained = manager.drain('kild-1', 'claude');
  expect(drained).toMatchObject({
    ok: true,
    value: { idle: false, capped: false, posts: [{ from: 'coder', text: 'review is done' }] },
  });
});

test('drain is destructive and the empty drain is the idle signal', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.sendAs('kild-1', 'coder', 'ping', ['claude']);

  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { idle: false } });
  const working = manager.liveKilds()[0]?.agents.find((p) => p.handle === 'claude');
  expect(working?.idle).toBe(false);

  // Second drain: nothing queued → empty, idle. No separate status verb exists, or needed.
  expect(manager.drain('kild-1', 'claude')).toEqual({
    ok: true,
    value: { posts: [], idle: true, capped: false },
  });
  const done = manager.liveKilds()[0]?.agents.find((p) => p.handle === 'claude');
  expect(done?.idle).toBe(true);
});

test('the wake cap stops a runaway sender from waking an attached agent forever', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  const wake = async () => {
    await manager.sendAs('kild-1', 'coder', 'again', ['claude']);
    const result = manager.drain('kild-1', 'claude');
    return result.ok ? result.value : undefined;
  };
  for (let turn = 0; turn < DEFAULT_WAKE_CAP; turn += 1) {
    expect(await wake()).toMatchObject({ idle: false });
  }
  // The cap trips: the harness is told nothing (so it may stop), the mail is not eaten.
  expect(await wake()).toMatchObject({ posts: [], idle: true, capped: true });
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { capped: false } });
});

test('owned delivery is untouched by the attached branch', async () => {
  const { manager, prompted } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }, { handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.sendAs('kild-1', 'orchestrator', 'do X', ['coder']);
  expect(prompted.at(-1)).toEqual({
    id: 's-2',
    text: '[#demo] @orchestrator: do X',
    from: 'orchestrator',
  });
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { posts: [] } });
});

test('a message with no addressee defaults to the lead and never fans out to inboxes', async () => {
  const { manager } = fixture();
  await newKild(manager, [{ handle: 'orchestrator' }, { handle: 'coder' }]);
  await manager.attach('kild-1', 'claude');
  await manager.sendFromHuman('kild-1', 'kick off');
  expect(manager.drain('kild-1', 'claude')).toMatchObject({ ok: true, value: { posts: [] } });
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
  await manager.sendAs('kild-1', 'coder', 'unread', ['claude']); // mail still queued
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
  await manager.sendFromHuman('kild-1', 'work');
  await manager.stop('kild-1');
  expect(manager.archived()[0]?.agents).toContainEqual(
    expect.objectContaining({ handle: 'claude', ownership: 'attached' }),
  );
});
