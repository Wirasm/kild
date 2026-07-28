import { randomUUID } from 'node:crypto';

import {
  createAgentSession,
  DefaultResourceLoader,
  getAgentDir,
  ModelRegistry,
  ModelRuntime,
  SessionManager as PiSessionManager,
} from '@earendil-works/pi-coding-agent';

import { configuredMemoryDir, configuredModels, resolvePluginPaths } from './kild/config.ts';
import {
  composeSessionTurn,
  DEFAULT_PROMPT,
  formatModelsSection,
  formatPersonasSection,
} from './kild/default-prompt.ts';
import { type RawAgentEvent, translate, type UiEvent } from './kild/events.ts';
import type { CommandAck, SendOut, SpawnOut, StopOut } from './kild/kild-types.ts';
import { projectMemorySection } from './kild/memory.ts';
import { resolveModel, withPersona } from './kild/models.ts';
import { listPersonas, resolvePersonaInstructions } from './kild/personas.ts';
import { createSendTool } from './kild/send-tool.ts';
import { createSpawnTool } from './kild/spawn-tool.ts';
import { createStopTool } from './kild/stop-tool.ts';
import { ensureWorktree } from './kild/worktree.ts';

/**
 * One agent session, one process. The engine spawns this (the same binary with
 * `KILD_ROLE=agent`) once per session — the coding-agent SDK keeps process-global
 * state and supports only a single session per process, so concurrency *requires* a
 * process per session. `UiEvent` JSONL (and, in a kild, `send` / `spawn` control
 * lines) go to stdout; prompt / stop commands are read from stdin.
 */
export async function runAgent(): Promise<never> {
  let cwd = process.env.KILD_CWD || process.cwd();
  const worktreeName = process.env.KILD_WORKTREE || undefined;
  const worktreeBase = process.env.KILD_BASE || undefined;
  const personaName = process.env.KILD_PERSONA || undefined;
  const modelPattern = process.env.KILD_MODEL || undefined;
  const inKild = !!process.env.KILD_KILD_ID;
  const skillsProfile = process.env.KILD_SKILLS_PROFILE || undefined;
  const forkFrom = process.env.KILD_FORK_SESSION || undefined;

  const emit = (event: UiEvent) => process.stdout.write(`${JSON.stringify(event)}\n`);
  const pendingCommands = new Map<
    string,
    { resolve: (text: string) => void; reject: (error: Error) => void }
  >();

  const emitKildCommand = <T extends SendOut | SpawnOut | StopOut>(command: T): Promise<string> => {
    const requestId = randomUUID();
    process.stdout.write(`${JSON.stringify({ ...command, requestId })}\n`);
    return new Promise<string>((resolve, reject) => {
      pendingCommands.set(requestId, { resolve, reject });
    });
  };

  // Optional isolation: run inside the named git worktree (create-or-attach) rather
  // than the raw repo. Done here (not in the manager) so spawn stays synchronous;
  // prompts sent before this resolves are OS-buffered on stdin, so none are lost.
  if (worktreeName) {
    try {
      cwd = (await ensureWorktree(cwd, worktreeName, worktreeBase)).path;
    } catch (err) {
      emit({ kind: 'error', message: `worktree: ${errText(err)}` });
      process.exit(1);
    }
  }

  // A given-but-unknown model errors (resolveModel throws); no model = pi default.
  let model: ReturnType<typeof resolveModel>;
  let session: Awaited<ReturnType<typeof createAgentSession>>['session'];
  try {
    // The canonical model/auth runtime (reads ~/.pi/agent auth.json + models.json);
    // ModelRegistry is the synchronous facade over it that resolveModel reads.
    const modelRuntime = await ModelRuntime.create();
    const registry = new ModelRegistry(modelRuntime);
    model = resolveModel(registry, modelPattern);
    // Every agent in a kild gets the same three tools — there is no rank, so there is no
    // tool only some agents hold. A non-kild session gets no custom tools.
    const customTools = inKild
      ? [
          createSendTool((to, text) => emitKildCommand({ kind: 'send', to, text })),
          createSpawnTool((spec) => emitKildCommand({ kind: 'spawn', ...spec })),
          createStopTool((spec) => emitKildCommand({ kind: 'stop', ...spec })),
        ]
      : undefined;
    // Skills: an explicit kild capability profile (KILD_SKILLS_PROFILE) is EXCLUSIVE — it
    // replaces pi's defaults with just that dir. Otherwise every session gets pi's defaults
    // PLUS the config-declared skill dirs, so a spawned agent can load a plugged-in skill when
    // the orchestrator tells it to. This is what makes a plugged-in framework's process
    // reachable by whoever gets spawned, not only the agent that was there first.
    let resourceLoader: DefaultResourceLoader | undefined;
    if (skillsProfile) {
      resourceLoader = new DefaultResourceLoader({
        cwd,
        agentDir: getAgentDir(),
        noSkills: true,
        additionalSkillPaths: [skillsProfile],
      });
    } else {
      const { skillDirs } = await resolvePluginPaths(cwd);
      resourceLoader = skillDirs.length
        ? new DefaultResourceLoader({
            cwd,
            agentDir: getAgentDir(),
            additionalSkillPaths: skillDirs,
          })
        : undefined;
    }
    await resourceLoader?.reload();
    // Fork-spawn: seed this session with the full history of an existing pi session
    // file (KILD_FORK_SESSION), frozen at fork time. `forkFrom` COPIES the source
    // into a brand-new session file (same semantics as pi's own `--fork`), so the
    // source is never written — two writers on one file are impossible.
    const piSessionManager = forkFrom ? PiSessionManager.forkFrom(forkFrom, cwd) : undefined;
    ({ session } = await createAgentSession({
      model,
      modelRuntime,
      cwd,
      customTools,
      resourceLoader,
      sessionManager: piSessionManager,
    }));
  } catch (err) {
    emit({ kind: 'error', message: errText(err) });
    process.exit(1);
  }
  if (model) emit({ kind: 'model', provider: model.provider, id: model.id });
  // The pi session's durable identity — lets an operator reopen this exact agent in the
  // terminal with `pi --session <file|id>` (the file path works from any cwd).
  emit({
    kind: 'pi_session',
    id: session.sessionManager.getSessionId(),
    file: session.sessionManager.getSessionFile(),
  });

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

  let preamble = personaName ? await resolvePersonaInstructions(personaName, cwd) : null;
  // Every session gets the generic mechanism guide (how to operate) on top of everything,
  // above the persona — so even a bare `general` session is competent. One-shot: it rides
  // only the first delivered turn. The kild-comms part is conditional inside the prompt.
  // A delegating (in-kild) session also gets the configured model catalog so it can pick
  // a model per fan-out agent.
  const modelsSection = inKild ? formatModelsSection(await configuredModels(cwd)) : '';
  // ...and the persona catalog, so it delegates to a persona that exists instead of
  // guessing a name and taking a rejection. Only for a session that can spawn.
  const personasSection = inKild ? formatPersonasSection(await listPersonas(cwd)) : '';
  // Persistent memory rides the first turn: every session gets the project's curated
  // memory + direction, read from the resolved memory dir (config `memory.dir`, default
  // `.kild/` — gitignored, so worktree checkouts never carry the default-dir files).
  const memorySections = [projectMemorySection(cwd, await configuredMemoryDir(cwd))]
    .filter(Boolean)
    .join('\n\n');
  let sessionPrefix: string | null = [
    DEFAULT_PROMPT,
    personasSection,
    modelsSection,
    memorySections,
  ]
    .filter(Boolean)
    .join('\n\n');

  // pi runs one turn at a time. A kild can deliver a message (prompt) to this agent
  // while it is still mid-turn, so we queue prompts and drain them strictly
  // sequentially rather than awaiting inside the stdin handler.
  const promptQueue: string[] = [];
  let draining = false;
  async function drainPrompts(): Promise<void> {
    if (draining) return;
    draining = true;
    while (promptQueue.length > 0) {
      const next = promptQueue.shift() as string;
      try {
        await session.prompt(next);
      } catch (err) {
        emit({ kind: 'error', message: errText(err) });
      }
    }
    draining = false;
  }

  let buf = '';
  process.stdin.on('data', (chunk: Buffer) => {
    buf += chunk.toString();
    const lines = buf.split('\n');
    buf = lines.pop() ?? ''; // keep the incomplete trailing line
    for (const raw of lines) {
      const line = raw.trim();
      if (!line) continue;
      let msg: {
        type?: string;
        text?: string;
        requestId?: string;
        result?: CommandAck['result'];
      };
      try {
        msg = JSON.parse(line) as typeof msg;
      } catch {
        continue; // a malformed command line must not crash the agent; skip it
      }
      if (msg.type === 'prompt' && msg.text) {
        // First delivered turn carries the preamble: the mechanism guide (how to operate)
        // on top of the persona (<persona>). Both ride only the first turn.
        promptQueue.push(composeSessionTurn(withPersona(msg.text, preamble), sessionPrefix));
        preamble = null;
        sessionPrefix = null;
        void drainPrompts();
      } else if (msg.type === 'stop') {
        for (const pending of pendingCommands.values()) pending.reject(new Error('agent stopped'));
        pendingCommands.clear();
        session.dispose();
        process.exit(0);
      } else if (msg.type === 'command_result' && msg.requestId && msg.result) {
        const pending = pendingCommands.get(msg.requestId);
        if (!pending) continue;
        pendingCommands.delete(msg.requestId);
        if (msg.result.ok) pending.resolve(msg.result.value.message);
        else pending.reject(new Error(msg.result.message));
      }
    }
  });
  process.stdin.on('end', () => {
    for (const pending of pendingCommands.values()) pending.reject(new Error('agent stdin ended'));
    pendingCommands.clear();
    session.dispose();
    process.exit(0);
  });

  // Keep the process alive on the stdin event loop until stop / EOF.
  return new Promise<never>(() => {});
}

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
