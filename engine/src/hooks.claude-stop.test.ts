import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * The Stop hook SCRIPT, executed as Claude Code executes it.
 *
 * Everything else about this feature was covered and this was not: the CLI verb
 * (`kild inbox --format claude-stop`) has tests, and the shell wrapper that decides what to
 * pass it had none. Nothing ran the file. It reads `session_id` off stdin with `sed`, gates
 * on `set -eu`, and chooses whether to pass `--session`/`--as` — none of which the TypeScript
 * tests touch, because they invoke the CLI directly.
 *
 * That gap is the shape of the thing: **verification code is exempted from gates because it
 * is not product, and the exemption is what lets it rot.** This script's own header warns
 * that a renamed verb "does not raise an error, it just stops working" — and the previous
 * generation of it did exactly that, silently, for months. It warned about itself while
 * nothing checked it.
 *
 * The contract, in one line, and every case here asserts it: print the drain's JSON, or print
 * NOTHING, and ALWAYS exit 0. A hook that cannot reach kild must never stop a turn.
 */

const HOOK = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'hooks',
  'claude-stop',
);
const CLI = path.join(path.dirname(fileURLToPath(import.meta.url)), 'cli.ts');

let engine: ReturnType<typeof Bun.serve> | undefined;
let engineUrl = '';
let kildHome = '';
let binDir = '';
/** What the stub answers a drain with. */
let drain: { messages: Array<{ from: string; text: string; ts: number }> } = { messages: [] };

beforeAll(async () => {
  kildHome = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'kild-hook-home-')));
  binDir = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'kild-hook-bin-')));

  engine = Bun.serve({
    port: 0,
    hostname: '127.0.0.1',
    fetch: () =>
      Response.json({ ok: true, idle: drain.messages.length === 0, capped: false, ...drain }),
  });
  engineUrl = `http://127.0.0.1:${engine.port}`;

  // The hook finds `kild` on PATH. Give it a real one that runs this checkout's CLI, so the
  // test exercises the actual wiring rather than a mock of it.
  const shim = path.join(binDir, 'kild');
  await fs.writeFile(shim, `#!/bin/sh\nexec bun run ${CLI} "$@"\n`);
  await fs.chmod(shim, 0o755);
});

afterAll(async () => {
  engine?.stop(true);
  await fs.rm(kildHome, { recursive: true, force: true });
  await fs.rm(binDir, { recursive: true, force: true });
});

/** Run the hook exactly as Claude Code does: payload on stdin, environment, nothing else. */
async function runHook(
  payload: unknown,
  env: Record<string, string> = {},
  opts: { onPath?: boolean } = {},
) {
  const proc = Bun.spawn(['sh', HOOK], {
    env: {
      PATH: `${opts.onPath === false ? '' : `${binDir}:`}${process.env.PATH ?? ''}`,
      HOME: process.env.HOME ?? '',
      KILD_ENGINE: engineUrl,
      KILD_HOME: kildHome,
      ...env,
    },
    stdin: new TextEncoder().encode(JSON.stringify(payload)),
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, exitCode };
}

/** Attach a session so the hook has a record to resolve, using the real CLI. */
async function attach(session: string, kildId: string, handle: string) {
  const proc = Bun.spawn(['bun', 'run', CLI, 'attach', kildId, '--as', handle], {
    env: {
      ...process.env,
      KILD_ENGINE: engineUrl,
      KILD_HOME: kildHome,
      CLAUDE_CODE_SESSION_ID: session,
    },
    stdout: 'pipe',
    stderr: 'pipe',
  });
  await proc.exited;
}

test('with mail waiting, the hook prints Stop JSON and exits 0', async () => {
  await attach('sess-hook-1', 'kild-h', 'claude');
  drain = { messages: [{ from: 'reviewer', text: 'PR is green', ts: 1 }] };

  const hook = await runHook({ session_id: 'sess-hook-1', hook_event_name: 'Stop' });
  expect(hook.exitCode).toBe(0);
  expect(JSON.parse(hook.stdout)).toMatchObject({
    decision: 'block',
    hookSpecificOutput: { hookEventName: 'Stop' },
  });
  drain = { messages: [] };
});

test('the session id is read from the PAYLOAD when the environment has none', async () => {
  // The whole reason the hook stopped discarding stdin. `--resume`/`--fork-session` mints a
  // new session id and a harness settings file can re-inject a stale kild id, so the payload
  // is the only channel that survives either.
  await attach('sess-hook-payload', 'kild-h', 'claude');
  drain = { messages: [{ from: 'agent', text: 'x', ts: 1 }] };

  const hook = await runHook({ session_id: 'sess-hook-payload' });
  expect(hook.exitCode).toBe(0);
  expect(hook.stdout).toContain('@agent');
  drain = { messages: [] };
});

test('with no mail, the hook prints NOTHING and exits 0', async () => {
  await attach('sess-hook-2', 'kild-h', 'claude');
  const hook = await runHook({ session_id: 'sess-hook-2' });
  expect(hook.stdout).toBe('');
  expect(hook.exitCode).toBe(0);
});

test('a session that never attached is silent, not an error', async () => {
  const hook = await runHook({ session_id: 'sess-never-attached' });
  expect(hook.stdout).toBe('');
  expect(hook.exitCode).toBe(0);
});

test('no session id anywhere: the hook does nothing at all, fast', async () => {
  const hook = await runHook({ hook_event_name: 'Stop' });
  expect(hook.stdout).toBe('');
  expect(hook.exitCode).toBe(0);
});

test('a DEAD engine is silence and exit 0 — never a blocked turn', async () => {
  await attach('sess-hook-dead', 'kild-h', 'claude');
  const hook = await runHook(
    { session_id: 'sess-hook-dead' },
    { KILD_ENGINE: 'http://127.0.0.1:1' },
  );
  expect(hook.stdout).toBe('');
  expect(hook.exitCode).toBe(0);
});

test('no kild on PATH is silence and exit 0', async () => {
  // A stale wiring on another machine must not fail somebody's every turn. A real PATH with
  // no kild on it — not an empty one, which would also lose the shell.
  const hook = await runHook({ session_id: 'sess-hook-1' }, { PATH: '/usr/bin:/bin' });
  expect(hook.stdout).toBe('');
  expect(hook.exitCode).toBe(0);
});

test('malformed JSON on stdin is silence and exit 0', async () => {
  const proc = Bun.spawn(['sh', HOOK], {
    env: {
      PATH: `${binDir}:${process.env.PATH ?? ''}`,
      KILD_ENGINE: engineUrl,
      KILD_HOME: kildHome,
    },
    stdin: new TextEncoder().encode('{not json at all'),
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [stdout, exitCode] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
  expect(stdout).toBe('');
  expect(exitCode).toBe(0);
});

test('the hook never writes to stdout except the block JSON', async () => {
  // stray chatter on a hook reads as a failure to the harness.
  await attach('sess-hook-quiet', 'kild-h', 'claude');
  const hook = await runHook({ session_id: 'sess-hook-quiet' });
  expect(hook.stdout).toBe('');
});
