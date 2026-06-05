import { AuthStorage, createAgentSession, ModelRegistry } from '@earendil-works/pi-coding-agent';

import { resolveAgentInstructions } from './kild/agents.ts';
import { type RawAgentEvent, translate, type UiEvent } from './kild/events.ts';

/**
 * One agent session, one process. The engine spawns this (the same binary with
 * `KILD_ROLE=worker`) once per session — the coding-agent SDK keeps process-global
 * state and supports only a single session per process, so concurrency *requires*
 * a process per session. Events are written to stdout as `UiEvent` JSONL; prompt
 * and stop commands are read from stdin as JSONL.
 */
export async function runWorker(): Promise<never> {
  const cwd = process.env.KILD_CWD || process.cwd();
  const agentName = process.env.KILD_AGENT || undefined;
  const modelPattern = process.env.KILD_MODEL || undefined;

  const emit = (event: UiEvent) => process.stdout.write(`${JSON.stringify(event)}\n`);

  const authStorage = AuthStorage.create();
  const registry = ModelRegistry.create(authStorage);
  const model = resolveModel(registry, modelPattern);

  const { session } = await createAgentSession({
    model,
    authStorage,
    modelRegistry: registry,
    cwd,
  });
  if (model) emit({ kind: 'model', provider: model.provider, id: model.id });

  session.subscribe((e: RawAgentEvent) => {
    const ui = translate(e);
    if (ui) emit(ui);
    if (e.type === 'agent_end') {
      const stats = session.getSessionStats();
      emit({
        kind: 'stats',
        tokens: stats.tokens.total,
        cost: stats.cost,
        context_pct: stats.contextUsage?.percent ?? null,
      });
    }
  });

  let preamble = agentName ? await resolveAgentInstructions(agentName, cwd) : null;

  let buf = '';
  process.stdin.on('data', async (chunk: Buffer) => {
    buf += chunk.toString();
    const lines = buf.split('\n');
    buf = lines.pop() ?? ''; // keep the incomplete trailing line
    for (const raw of lines) {
      const line = raw.trim();
      if (!line) continue;
      const msg = JSON.parse(line) as { type: string; text?: string };
      if (msg.type === 'prompt' && msg.text) {
        let text = msg.text;
        if (preamble) {
          text = `<role>\n${preamble}\n</role>\n\n${text}`;
          preamble = null;
        }
        try {
          await session.prompt(text);
        } catch (err) {
          console.error('worker prompt error:', err);
          emit({ kind: 'agent_end' });
        }
      } else if (msg.type === 'stop') {
        session.dispose();
        process.exit(0);
      }
    }
  });
  process.stdin.on('end', () => {
    session.dispose();
    process.exit(0);
  });

  // Keep the process alive on the stdin event loop until stop / EOF.
  return new Promise<never>(() => {});
}

function resolveModel(registry: ModelRegistry, pattern?: string) {
  if (!pattern) return undefined;
  const slash = pattern.indexOf('/');
  if (slash !== -1) return registry.find(pattern.slice(0, slash), pattern.slice(slash + 1));
  return registry.getAll().find((m) => m.id === pattern);
}
