import fs from 'node:fs/promises';
import path from 'node:path';

import { kildHome, resolvePluginPaths } from './config.ts';

/** A reusable persona: a name + system prompt, read from convention dirs. The on-disk
 *  dir names (`.pi/agents`, `.claude/agents`, `.kild/agents`) are upstream pi/Claude
 *  convention and deliberately NOT renamed to "personas" — kild only reads what those
 *  ecosystems already write. Mirror of kild-core::agent. */
export interface Persona {
  name: string;
  /** From frontmatter `description:` — the discovery signal an orchestrator reads
   *  to know what a persona is for. Empty string when the file has none. */
  description: string;
  systemPrompt: string;
}

/**
 * The built-in persona every project has without writing a file.
 *
 * Named `general` rather than `default` because the name is read by an agent choosing who to
 * delegate to, and "default" describes its place in the system rather than what it is —
 * "spawn `default`" says nothing about what you would get. It is still what you get when no
 * persona is named; that is a fact about the engine, not a good name for a capability.
 *
 * Its `systemPrompt` is empty on purpose: the baseline in `default-prompt.ts` already makes a
 * bare agent competent, so there is nothing to layer on top. A persona file named
 * `general.md` does NOT shadow it — the built-in is seeded first and wins, so the one persona
 * that is always available cannot be quietly replaced.
 */
export const GENERAL_PERSONA = 'general';

/** What the general agent is for, as a delegating agent reads it in the catalog. */
export const GENERAL_DESCRIPTION =
  'General-purpose, no specialisation. Use when no other persona fits — it can take on ' +
  'anything, given a clear goal, the outcome you want, and how you will judge it done.';

async function personaDirs(projectRoot?: string): Promise<string[]> {
  const dirs: string[] = [];
  if (projectRoot) {
    dirs.push(path.join(projectRoot, '.kild/agents'));
    dirs.push(path.join(projectRoot, '.claude/agents'));
    dirs.push(path.join(projectRoot, '.pi/agents'));
    // Config-declared plugins/agentPaths — how the project brings its own personas.
    dirs.push(...(await resolvePluginPaths(projectRoot)).agentDirs);
  }
  dirs.push(path.join(kildHome(), 'agents'));
  const home = process.env.HOME;
  if (home) dirs.push(path.join(home, '.claude/agents'));
  return dirs;
}

/** Split a leading YAML frontmatter block from the body. Returns the raw
 *  frontmatter (without the `---` fences) and the trimmed system-prompt body. */
function splitFrontmatter(content: string): { frontmatter: string; body: string } {
  const normalized = content.replace(/\r\n/g, '\n');
  if (normalized.startsWith('---\n')) {
    const end = normalized.indexOf('\n---', 4);
    if (end !== -1) {
      return { frontmatter: normalized.slice(4, end), body: normalized.slice(end + 4).trim() };
    }
  }
  return { frontmatter: '', body: content.trim() };
}

/** Strip a leading YAML frontmatter block (if any) — the persona's system prompt. */
export function stripFrontmatter(content: string): string {
  return splitFrontmatter(content).body;
}

/** The `description:` value from a frontmatter block (quotes trimmed), or ''. */
function frontmatterDescription(frontmatter: string): string {
  for (const line of frontmatter.split('\n')) {
    const idx = line.indexOf(':');
    if (idx > 0 && line.slice(0, idx).trim() === 'description') {
      return line
        .slice(idx + 1)
        .trim()
        .replace(/^['"]|['"]$/g, '');
    }
  }
  return '';
}

/** The built-in `general` persona + every `<name>.md` across convention dirs (first wins). */
export async function listPersonas(projectRoot?: string): Promise<Persona[]> {
  const personas: Persona[] = [
    { name: GENERAL_PERSONA, description: GENERAL_DESCRIPTION, systemPrompt: '' },
  ];
  const seen = new Set<string>([GENERAL_PERSONA]);

  for (const dir of await personaDirs(projectRoot)) {
    let entries: string[];
    try {
      entries = await fs.readdir(dir);
    } catch {
      continue; // missing dir is fine
    }
    for (const entry of entries.sort()) {
      if (!entry.endsWith('.md')) continue;
      const name = entry.slice(0, -3);
      if (seen.has(name)) continue;
      const content = await fs.readFile(path.join(dir, entry), 'utf8').catch(() => null);
      if (content === null) continue;
      seen.add(name);
      const { frontmatter, body } = splitFrontmatter(content);
      personas.push({ name, description: frontmatterDescription(frontmatter), systemPrompt: body });
    }
  }
  return personas;
}

/** The instructions to layer for a chosen persona (null = pi's own default). */
export async function resolvePersonaInstructions(
  name: string,
  projectRoot?: string,
): Promise<string | null> {
  if (name === GENERAL_PERSONA) return null;
  const persona = (await listPersonas(projectRoot)).find((p) => p.name === name);
  return persona?.systemPrompt ? persona.systemPrompt : null;
}
