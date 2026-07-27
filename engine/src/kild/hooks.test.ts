import { afterAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  type HookAgentSpawn,
  hookEnv,
  type KildCloseEvent,
  renderHookText,
  runCloseHook,
} from './hooks.ts';

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-hooks-'));
afterAll(() => fs.rmSync(tmp, { recursive: true, force: true }));

function event(overrides: Partial<KildCloseEvent> = {}): KildCloseEvent {
  return {
    kildId: 'kild-1',
    name: 'fix-auth',
    cwd: tmp,
    worktree: 'fix-auth',
    base: 'main',
    transcriptPath: '/home/kilds/kild-1.json',
    ledgerPath: path.join(tmp, '.kild', 'LOG.md'),
    ...overrides,
  };
}

function spawnRecorder() {
  const calls: Array<{ spec: HookAgentSpawn; prompt: string }> = [];
  return { calls, spawn: (spec: HookAgentSpawn, prompt: string) => calls.push({ spec, prompt }) };
}

test('hook text substitutes every close fact and leaves unknown placeholders alone', () => {
  const text = renderHookText(
    'kild {{name}} ({{kildId}}) in {{cwd}} on {{worktree}}/{{base}}: ' +
      '{{transcriptPath}} {{ledgerPath}} {{notAFact}}',
    event(),
  );
  expect(text).toBe(
    `kild fix-auth (kild-1) in ${tmp} on fix-auth/main: ` +
      `/home/kilds/kild-1.json ${path.join(tmp, '.kild', 'LOG.md')} {{notAFact}}`,
  );
});

test('an absent fact leaves its placeholder and its env var out', () => {
  const bare = event({ worktree: undefined, base: undefined });
  expect(renderHookText('tree={{worktree}}', bare)).toBe('tree={{worktree}}');
  expect(hookEnv(bare).KILD_CLOSE_WORKTREE).toBeUndefined();
});

test('close facts reach a command hook as KILD_CLOSE_* environment', () => {
  expect(hookEnv(event())).toMatchObject({
    KILD_CLOSE_KILD_ID: 'kild-1',
    KILD_CLOSE_NAME: 'fix-auth',
    KILD_CLOSE_BASE: 'main',
    KILD_CLOSE_TRANSCRIPT_PATH: '/home/kilds/kild-1.json',
  });
});

test('an agent hook spawns one agent with its persona, model and rendered prompt', async () => {
  const recorder = spawnRecorder();
  await runCloseHook(
    {
      agent: {
        persona: 'archivist',
        model: 'openai-codex/gpt-5.6-sol',
        prompt: 'read {{name}}',
      },
    },
    event(),
    recorder.spawn,
  );

  expect(recorder.calls).toHaveLength(1);
  expect(recorder.calls[0]?.prompt).toBe('read fix-auth');
  expect(recorder.calls[0]?.spec).toEqual({
    model: 'openai-codex/gpt-5.6-sol',
    // The MAIN checkout, never the worktree — it may be pruned after the stop.
    cwd: tmp,
    persona: 'archivist',
    label: 'onClose:fix-auth',
  });
});

test('an agent hook without a persona runs as `default`', async () => {
  const recorder = spawnRecorder();
  await runCloseHook({ agent: { prompt: 'go' } }, event(), recorder.spawn);
  expect(recorder.calls[0]?.spec.persona).toBe('default');
});

test('a command hook runs in the kild cwd with the facts in its environment', async () => {
  const recorder = spawnRecorder();
  await runCloseHook(
    {
      command: ['sh', '-c', 'printf "%s %s" "$KILD_CLOSE_NAME" "$1" > fired.txt', 'sh', '{{base}}'],
    },
    event(),
    recorder.spawn,
  );

  expect(fs.readFileSync(path.join(tmp, 'fired.txt'), 'utf8')).toBe('fix-auth main');
  expect(recorder.calls).toHaveLength(0); // nothing spawned: the hook declared no agent
});

test('a failing command hook is logged, never thrown — a stop cannot break on it', async () => {
  const recorder = spawnRecorder();
  await runCloseHook({ command: ['sh', '-c', 'exit 3'] }, event(), recorder.spawn);
  await runCloseHook({ command: ['definitely-not-a-binary'] }, event(), recorder.spawn);
});

test('an empty hook does nothing at all', async () => {
  const recorder = spawnRecorder();
  await runCloseHook({}, event(), recorder.spawn);
  await runCloseHook({ command: [] }, event(), recorder.spawn);
  expect(recorder.calls).toHaveLength(0);
});
