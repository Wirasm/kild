import { beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * `POST /api/kilds` must refuse a cwd that does not exist.
 *
 * The regression: a caller resolving an unregistered `--project NAME` against its own
 * cwd produced a well-formed ABSOLUTE path to a directory that was never there
 * (`…/sild/helm` + `helm` → `…/sild/helm/helm`). `isAbsolute` passed, the kild was created,
 * its worktree could not be created, no agent spawned — and the caller got a kild
 * id and exit 0. Silence is the worst failure mode for a delegation.
 *
 * Against the real Hono app via `fetch`; no port is bound, so the operator's engine on
 * 4517 is untouched.
 */
let fetchApp: (req: Request) => Response | Promise<Response>;

beforeAll(async () => {
  process.env.KILD_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-cwd-'));
  const server = (await import('./server.ts')).default;
  fetchApp = server.fetch as typeof fetchApp;
});

function newKild(body: unknown): Promise<Response> {
  return Promise.resolve(
    fetchApp(
      new Request('http://localhost/api/kilds', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      }),
    ),
  );
}

const base = {
  name: 'r',
  kickoff: { to: ['agent'], text: 'do a thing' },
  agents: [{ handle: 'agent' }],
};

test('an absolute cwd that does not exist is refused', async () => {
  const missing = path.join(os.tmpdir(), `kild-not-here-${Date.now()}`);
  const res = await newKild({ ...base, cwd: missing });
  expect(res.status).toBe(400);
  expect((await res.json()) as { error: string }).toEqual({
    error: `cwd is not an existing directory: ${missing}`,
  });
});

test('an absolute cwd that is a FILE, not a directory, is refused', async () => {
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'kild-cwd-file-')), 'a.txt');
  fs.writeFileSync(file, 'x');
  const res = await newKild({ ...base, cwd: file });
  expect(res.status).toBe(400);
  expect(((await res.json()) as { error: string }).error).toContain('not an existing directory');
});

test('a relative cwd is still refused first, with the absolute-path message', async () => {
  const res = await newKild({ ...base, cwd: 'helm/helm' });
  expect(res.status).toBe(400);
  expect(((await res.json()) as { error: string }).error).toContain('must be absolute');
});

test('an existing directory passes the cwd check', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-cwd-ok-'));
  const res = await newKild({ ...base, cwd: dir });
  // It may still fail further along (spawning a real session needs pi), but it must
  // never fail ON THE CWD.
  const body = await res.text();
  expect(body).not.toContain('not an existing directory');
  expect(body).not.toContain('must be absolute');
});
