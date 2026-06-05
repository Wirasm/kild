import { AuthStorage, createAgentSession, ModelRegistry } from '@earendil-works/pi-coding-agent';

import { resolveAgentInstructions } from './kild/agents.ts';
import { type RawAgentEvent, translate, type UiEvent } from './kild/events.ts';
import { resolveModel, withRole } from './kild/models.ts';
import { ensureWorktree } from './kild/worktree.ts';

/**
 * One agent session, one process. The engine spawns this (the same binary with
 * `KILD_ROLE=worker`) once per session — the coding-agent SDK keeps process-global
 * state and supports only a single session per process, so concurrency *requires*
 * a process per session. Events are written to stdout as `UiEvent` JSONL; prompt
 * and stop commands are read from stdin as JSONL.
 */
export async function runWorker(): Promise<never> {
  let cwd = process.env.KILD_CWD || process.cwd();
  const worktreeName = process.env.KILD_WORKTREE || undefined;
  const agentName = process.env.KILD_AGENT || undefined;
  const modelPattern = process.env.KILD_MODEL || undefined;

  const emit = (event: UiEvent) => process.stdout.write(`${JSON.stringify(event)}\n`);

  // Optional isolation: run inside the named git worktree (create-or-attach) rather
  // than the raw repo. Done here (not in the manager) so spawn stays synchronous;
  // prompts sent before this resolves are OS-buffered on stdin, so none are lost.
  if (worktreeName) {
    try {
      cwd = (await ensureWorktree(cwd, worktreeName)).path;
    } catch (err) {
      emit({ kind: 'error', message: `worktree: ${errText(err)}` });
      process.exit(1);
    }
  }

  const authStorage = AuthStorage.create();
  const registry = ModelRegistry.create(authStorage);

  // A given-but-unknown model errors (resolveModel throws); no model = pi default.
  let model: ReturnType<typeof resolveModel>;
  let session: Awaited<ReturnType<typeof createAgentSession>>['session'];
  try {
    model = resolveModel(registry, modelPattern);
    ({ session } = await createAgentSession({ model, authStorage, modelRegistry: registry, cwd }));
  } catch (err) {
    emit({ kind: 'error', message: errText(err) });
    process.exit(1);
  }
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
      let msg: { type: string; text?: string };
      try {
        msg = JSON.parse(line) as { type: string; text?: string };
      } catch {
        continue; // a malformed command line must not crash the worker; skip it
      }
      if (msg.type === 'prompt' && msg.text) {
        const text = withRole(msg.text, preamble);
        preamble = null;
        try {
          await session.prompt(text);
        } catch (err) {
          emit({ kind: 'error', message: errText(err) });
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

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
