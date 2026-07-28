import { afterAll, beforeAll, expect, test } from 'bun:test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * `kild send` and the CLI's one addressing convenience.
 *
 * The ENGINE never defaults a recipient. The CLI may, because it can see the roster and
 * a kild with exactly one agent has no ambiguity to resolve — but it resolves it
 * CLIENT-side and puts the handle on the wire. These tests pin that split: whatever the
 * user typed, the request the engine receives always names its recipients.
 *
 * Every case runs the real CLI against a throwaway engine on an ephemeral port. It never
 * touches the operator's engine on 4517.
 */

const CLI = path.join(path.dirname(fileURLToPath(import.meta.url)), 'cli.ts');

let engine: ReturnType<typeof Bun.serve> | undefined;
let engineUrl = '';
let requests: Array<{ method: string; path: string; body: unknown }> = [];
/** The roster `GET /api/kilds` reports — set per test. */
let roster: string[] = ['agent'];

beforeAll(() => {
  engine = Bun.serve({
    port: 0, // ephemeral — never the operator's 4517
    hostname: '127.0.0.1',
    fetch: async (request) => {
      const url = new URL(request.url);
      requests.push({
        method: request.method,
        path: url.pathname,
        body: await request.json().catch(() => undefined),
      });
      if (url.pathname === '/api/kilds' && request.method === 'GET') {
        return Response.json([
          {
            id: 'kild-1',
            name: 'demo',
            agents: roster.map((handle) => ({ handle })),
            log: [],
          },
        ]);
      }
      return Response.json({ ok: true, message: 'Sent to the kild.' });
    },
  });
  engineUrl = `http://127.0.0.1:${engine.port}`;
});

afterAll(() => engine?.stop(true));

async function runCli(args: string[]) {
  const proc = Bun.spawn(['bun', 'run', CLI, ...args], {
    env: { ...process.env, KILD_ENGINE: engineUrl },
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

const sends = () => requests.filter((r) => r.path === '/api/kilds/kild-1/messages');

test('--to is passed through verbatim, one handle or several', async () => {
  requests = [];
  expect((await runCli(['send', 'kild-1', '--to', 'coder,@reviewer', 'status?'])).exitCode).toBe(0);
  expect(sends()).toEqual([
    {
      method: 'POST',
      path: '/api/kilds/kild-1/messages',
      body: { to: ['coder', 'reviewer'], text: 'status?' },
    },
  ]);
});

test('without --to in a ONE-agent kild the CLI resolves the handle and sends it explicitly', async () => {
  requests = [];
  roster = ['agent'];
  expect((await runCli(['send', 'kild-1', 'fix the bug'])).exitCode).toBe(0);
  // The convenience is the CLI's; the engine still receives a named recipient.
  expect(sends()).toEqual([
    {
      method: 'POST',
      path: '/api/kilds/kild-1/messages',
      body: { to: ['agent'], text: 'fix the bug' },
    },
  ]);
});

test('without --to in a MULTI-agent kild the CLI refuses and names the roster', async () => {
  requests = [];
  roster = ['coder', 'reviewer'];
  const { exitCode, stderr } = await runCli(['send', 'kild-1', 'status?']);
  expect(exitCode).toBe(1);
  expect(stderr).toContain('--to is required');
  expect(stderr).toContain('coder, reviewer');
  expect(sends()).toEqual([]); // nothing was sent on a guess
  roster = ['agent'];
});

test('an empty --to is a usage error, never "everyone"', async () => {
  requests = [];
  const { exitCode, stderr } = await runCli(['send', 'kild-1', '--to', ' , ', 'status?']);
  expect(exitCode).toBe(1);
  expect(stderr).toContain('--to must name at least one agent');
  expect(sends()).toEqual([]);
});
