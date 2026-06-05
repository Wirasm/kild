import fs from 'node:fs/promises';
import path from 'node:path';

/** A reusable role: a name + system prompt, read from convention dirs.
 *  Mirror of kild-core::agent (same .kild/.claude/.pi discovery). */
export interface Agent {
  name: string;
  systemPrompt: string;
}

export const DEFAULT_AGENT = 'default';

function agentDirs(projectRoot?: string): string[] {
  const dirs: string[] = [];
  if (projectRoot) {
    dirs.push(path.join(projectRoot, '.kild/agents'));
    dirs.push(path.join(projectRoot, '.claude/agents'));
    dirs.push(path.join(projectRoot, '.pi/agents'));
  }
  const home = process.env.HOME;
  if (home) dirs.push(path.join(home, '.claude/agents'));
  return dirs;
}

/** Strip a leading YAML frontmatter block (if any). */
export function stripFrontmatter(content: string): string {
  const normalized = content.replace(/\r\n/g, '\n');
  if (normalized.startsWith('---\n')) {
    const end = normalized.indexOf('\n---', 4);
    if (end !== -1) return normalized.slice(end + 4).trim();
  }
  return content.trim();
}

/** Built-in `default` + every `<name>.md` across convention dirs (first wins). */
export async function listAgents(projectRoot?: string): Promise<Agent[]> {
  const agents: Agent[] = [{ name: DEFAULT_AGENT, systemPrompt: '' }];
  const seen = new Set<string>([DEFAULT_AGENT]);

  for (const dir of agentDirs(projectRoot)) {
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
      agents.push({ name, systemPrompt: stripFrontmatter(content) });
    }
  }
  return agents;
}

/** The instructions to layer for a chosen agent (null = pi's own default). */
export async function resolveAgentInstructions(
  name: string,
  projectRoot?: string,
): Promise<string | null> {
  if (name === DEFAULT_AGENT) return null;
  const agent = (await listAgents(projectRoot)).find((a) => a.name === name);
  return agent && agent.systemPrompt ? agent.systemPrompt : null;
}
