import fs from 'node:fs/promises';
import path from 'node:path';

/** A reusable role: a name + system prompt, read from convention dirs.
 *  Mirror of kild-core::agent (same .kild/.claude/.pi discovery). */
export interface Agent {
  name: string;
  /** From frontmatter `description:` — the discovery signal an orchestrator reads
   *  to know what an agent is for. Empty string when the file has none. */
  description: string;
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

/** Strip a leading YAML frontmatter block (if any) — the agent's system prompt. */
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

/** Built-in `default` + every `<name>.md` across convention dirs (first wins). */
export async function listAgents(projectRoot?: string): Promise<Agent[]> {
  const agents: Agent[] = [{ name: DEFAULT_AGENT, description: '', systemPrompt: '' }];
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
      const { frontmatter, body } = splitFrontmatter(content);
      agents.push({ name, description: frontmatterDescription(frontmatter), systemPrompt: body });
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
  return agent?.systemPrompt ? agent.systemPrompt : null;
}
