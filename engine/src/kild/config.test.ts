import { afterEach, beforeEach, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { resolvePluginPaths } from './config.ts';

let tmp: string;
let prevHome: string | undefined;

beforeEach(() => {
  prevHome = process.env.KILD_HOME;
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-config-'));
  process.env.KILD_HOME = path.join(tmp, 'home');
  fs.mkdirSync(process.env.KILD_HOME, { recursive: true });
});

afterEach(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function writeProjectConfig(proj: string, cfg: unknown): void {
  fs.mkdirSync(path.join(proj, '.kild'), { recursive: true });
  fs.writeFileSync(path.join(proj, '.kild', 'config.json'), JSON.stringify(cfg));
}

test('a plugin contributes its agents/ and skills/ subdirs (relative to the config)', async () => {
  const proj = path.join(tmp, 'proj');
  writeProjectConfig(proj, { plugins: ['./prp-core'] });
  const { agentDirs, skillDirs } = await resolvePluginPaths(proj);
  expect(agentDirs).toContain(path.join(proj, 'prp-core', 'agents'));
  expect(skillDirs).toContain(path.join(proj, 'prp-core', 'skills'));
});

test('absolute and ~ paths load from anywhere on the system', async () => {
  const proj = path.join(tmp, 'proj');
  writeProjectConfig(proj, { skillPaths: ['/opt/skills'], agentPaths: ['~/my-agents'] });
  const { agentDirs, skillDirs } = await resolvePluginPaths(proj);
  expect(skillDirs).toContain('/opt/skills');
  expect(agentDirs).toContain(path.join(process.env.HOME ?? '', 'my-agents'));
});

test('global ($KILD_HOME) and project configs both contribute', async () => {
  fs.writeFileSync(
    path.join(process.env.KILD_HOME as string, 'config.json'),
    JSON.stringify({ skillPaths: ['/global/skills'] }),
  );
  const proj = path.join(tmp, 'proj');
  writeProjectConfig(proj, { skillPaths: ['./local/skills'] });
  const { skillDirs } = await resolvePluginPaths(proj);
  expect(skillDirs).toContain('/global/skills');
  expect(skillDirs).toContain(path.join(proj, 'local', 'skills'));
});

test('missing or malformed config yields nothing and never throws', async () => {
  const proj = path.join(tmp, 'noconfig');
  fs.mkdirSync(proj, { recursive: true });
  expect(await resolvePluginPaths(proj)).toEqual({ agentDirs: [], skillDirs: [] });

  writeProjectConfig(proj, 'not an object');
  expect(await resolvePluginPaths(proj)).toEqual({ agentDirs: [], skillDirs: [] });
});
