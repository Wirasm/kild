import { afterEach, beforeEach, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { configuredCloseHook, configuredMemoryDir, resolvePluginPaths } from './config.ts';

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

test('memory.dir defaults to .kild resolved against the project cwd', async () => {
  const proj = path.join(tmp, 'proj-mem-default');
  fs.mkdirSync(proj, { recursive: true });
  expect(await configuredMemoryDir(proj)).toBe(path.join(proj, '.kild'));
});

test('a relative memory.dir resolves against the project cwd', async () => {
  const proj = path.join(tmp, 'proj-mem-rel');
  writeProjectConfig(proj, { memory: { dir: 'notes/kild' } });
  expect(await configuredMemoryDir(proj)).toBe(path.join(proj, 'notes', 'kild'));
});

test('an absolute memory.dir is taken as-is and ~ expands to $HOME', async () => {
  const proj = path.join(tmp, 'proj-mem-abs');
  writeProjectConfig(proj, { memory: { dir: '/stores/proj-a' } });
  expect(await configuredMemoryDir(proj)).toBe('/stores/proj-a');

  const proj2 = path.join(tmp, 'proj-mem-home');
  writeProjectConfig(proj2, { memory: { dir: '~/stores/proj-a' } });
  expect(await configuredMemoryDir(proj2)).toBe(
    path.join(process.env.HOME ?? '', 'stores', 'proj-a'),
  );
});

test('project memory.dir wins over global; global applies when the project sets none', async () => {
  fs.writeFileSync(
    path.join(process.env.KILD_HOME as string, 'config.json'),
    JSON.stringify({ memory: { dir: '/global/store' } }),
  );
  const proj = path.join(tmp, 'proj-mem-merge');
  writeProjectConfig(proj, { memory: { dir: '/project/store' } });
  expect(await configuredMemoryDir(proj)).toBe('/project/store');

  const bare = path.join(tmp, 'proj-mem-bare');
  fs.mkdirSync(bare, { recursive: true });
  expect(await configuredMemoryDir(bare)).toBe('/global/store');
});

test('hooks.onClose is read back verbatim — the engine never interprets it', async () => {
  const proj = path.join(tmp, 'proj-hook');
  const hook = {
    agent: {
      model: 'openai-codex/gpt-5.6-sol',
      persona: 'archivist',
      prompt: 'read {{ledgerPath}}',
    },
  };
  writeProjectConfig(proj, { hooks: { onClose: hook } });
  expect(await configuredCloseHook(proj)).toEqual(hook);
});

test('a project hooks.onClose replaces the global one; global applies when absent', async () => {
  fs.writeFileSync(
    path.join(process.env.KILD_HOME as string, 'config.json'),
    JSON.stringify({ hooks: { onClose: { command: ['global-hook'] } } }),
  );
  const proj = path.join(tmp, 'proj-hook-merge');
  writeProjectConfig(proj, { hooks: { onClose: { command: ['project-hook'] } } });
  expect(await configuredCloseHook(proj)).toEqual({ command: ['project-hook'] });

  const bare = path.join(tmp, 'proj-hook-bare');
  fs.mkdirSync(bare, { recursive: true });
  expect(await configuredCloseHook(bare)).toEqual({ command: ['global-hook'] });
});

test('no declared hook is undefined — close then emits its event and runs nothing', async () => {
  const proj = path.join(tmp, 'proj-nohook');
  fs.mkdirSync(proj, { recursive: true });
  expect(await configuredCloseHook(proj)).toBeUndefined();
});

test('missing or malformed config yields nothing and never throws', async () => {
  const proj = path.join(tmp, 'noconfig');
  fs.mkdirSync(proj, { recursive: true });
  expect(await resolvePluginPaths(proj)).toEqual({ agentDirs: [], skillDirs: [] });

  writeProjectConfig(proj, 'not an object');
  expect(await resolvePluginPaths(proj)).toEqual({ agentDirs: [], skillDirs: [] });
  expect(await configuredMemoryDir(proj)).toBe(path.join(proj, '.kild')); // falls back to default
});
