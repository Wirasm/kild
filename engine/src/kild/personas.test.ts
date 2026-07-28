import { expect, test } from 'bun:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { listPersonas, stripFrontmatter } from './personas.ts';

test('strips a leading YAML frontmatter block', () => {
  expect(
    stripFrontmatter('---\nname: planner\ndescription: plans\n---\nYou are a planner.\n'),
  ).toBe('You are a planner.');
});

test('passes through content with no frontmatter', () => {
  expect(stripFrontmatter('You are a planner.')).toBe('You are a planner.');
});

test('normalizes CRLF', () => {
  expect(stripFrontmatter('---\r\nname: x\r\n---\r\nbody')).toBe('body');
});

test('discovers scoped global agents before home Claude agents', async () => {
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'kild-agents-'));
  const projectRoot = path.join(tempDir, 'project');
  const home = path.join(tempDir, 'home');
  const scopedHome = path.join(tempDir, 'kild-home');
  const previousHome = process.env.HOME;
  const previousKildHome = process.env.KILD_HOME;

  const writeAgent = async (dir: string, name: string, prompt: string) => {
    await fs.mkdir(dir, { recursive: true });
    await fs.writeFile(path.join(dir, `${name}.md`), prompt);
  };

  try {
    await writeAgent(path.join(projectRoot, '.kild/agents'), 'project', 'project kild');
    await writeAgent(path.join(projectRoot, '.claude/agents'), 'claude', 'project claude');
    await writeAgent(path.join(projectRoot, '.pi/agents'), 'pi', 'project pi');
    await writeAgent(path.join(scopedHome, 'agents'), 'global', 'scoped global');
    await writeAgent(path.join(scopedHome, 'agents'), 'claude', 'scoped global claude');
    await writeAgent(path.join(home, '.claude/agents'), 'global', 'home global');
    await writeAgent(path.join(home, '.claude/agents'), 'home', 'home claude');
    process.env.HOME = home;
    process.env.KILD_HOME = scopedHome;

    const personas = await listPersonas(projectRoot);

    expect(personas.map((persona) => persona.name)).toEqual([
      'general',
      'project',
      'claude',
      'pi',
      'global',
      'home',
    ]);
    expect(personas.find((persona) => persona.name === 'claude')?.systemPrompt).toBe(
      'project claude',
    );
    expect(personas.find((persona) => persona.name === 'global')?.systemPrompt).toBe(
      'scoped global',
    );
  } finally {
    if (previousHome === undefined) delete process.env.HOME;
    else process.env.HOME = previousHome;
    if (previousKildHome === undefined) delete process.env.KILD_HOME;
    else process.env.KILD_HOME = previousKildHome;
    await fs.rm(tempDir, { recursive: true, force: true });
  }
});
