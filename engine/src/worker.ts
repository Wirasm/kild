import { randomUUID } from 'node:crypto';

import {
  AuthStorage,
  createAgentSession,
  DefaultResourceLoader,
  getAgentDir,
  ModelRegistry,
} from '@earendil-works/pi-coding-agent';

import { resolveAgentInstructions } from './kild/agents.ts';
import { configuredModels, resolvePluginPaths } from './kild/config.ts';
import { type RawAgentEvent, translate, type UiEvent } from './kild/events.ts';
import { createFleetCloseRoomTool } from './kild/fleet/close-room-tool.ts';
import { createOpenRoomTool } from './kild/fleet/open-room-tool.ts';
import { createPostRoomTool } from './kild/fleet/post-room-tool.ts';
import { createRoomsStatusTool } from './kild/fleet/rooms-status-tool.ts';
import {
  composeSessionTurn,
  formatModelsSection,
  MECHANISM_PROMPT,
} from './kild/mechanism-prompt.ts';
import { resolveModel, withRole } from './kild/models.ts';
import { createCloseRoomTool } from './kild/room/close-room-tool.ts';
import { createInviteAgentTool } from './kild/room/invite-agent-tool.ts';
import { createPostMessageTool } from './kild/room/post-message-tool.ts';
import type {
  CloseRoomOut,
  InviteOut,
  MessageOut,
  RoomCommandAck,
} from './kild/room/room-types.ts';
import { ensureWorktree } from './kild/worktree.ts';

/**
 * One agent session, one process. The engine spawns this (the same binary with
 * `KILD_ROLE=worker`) once per session — the coding-agent SDK keeps process-global
 * state and supports only a single session per process, so concurrency *requires* a
 * process per session. `UiEvent` JSONL (and, in a room, `message_out` / `invite`
 * control lines) go to stdout; prompt / stop commands are read from stdin.
 */
export async function runWorker(): Promise<never> {
  let cwd = process.env.KILD_CWD || process.cwd();
  const worktreeName = process.env.KILD_WORKTREE || undefined;
  const worktreeBase = process.env.KILD_BASE || undefined;
  const agentName = process.env.KILD_AGENT || undefined;
  const modelPattern = process.env.KILD_MODEL || undefined;
  const inRoom = !!process.env.KILD_ROOM;
  const isRoomLead = process.env.KILD_ROOM_LEAD === '1';
  const fleetEnabled = process.env.KILD_FLEET === '1';
  const skillsProfile = process.env.KILD_SKILLS_PROFILE || undefined;

  const emit = (event: UiEvent) => process.stdout.write(`${JSON.stringify(event)}\n`);
  const pendingCommands = new Map<
    string,
    { resolve: (text: string) => void; reject: (error: Error) => void }
  >();

  const emitRoomCommand = <T extends MessageOut | InviteOut | CloseRoomOut>(
    command: T,
  ): Promise<string> => {
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

  const authStorage = AuthStorage.create();
  const registry = ModelRegistry.create(authStorage);

  // Per-turn state for the implicit-reply rule (room only): the turn's sender, whether
  // the agent posted explicitly this turn, and its accumulated final text. Reset at
  // the start of each turn (in drainPrompts).
  let turnSender = 'human';
  let postedThisTurn = false;
  let turnText = '';

  // A given-but-unknown model errors (resolveModel throws); no model = pi default.
  let model: ReturnType<typeof resolveModel>;
  let session: Awaited<ReturnType<typeof createAgentSession>>['session'];
  try {
    model = resolveModel(registry, modelPattern);
    // A room participant gets `post_message` + `invite_agent`; the room's LEAD also
    // gets `close_room` (ending the room is the lead's explicit act). A fleet-enabled
    // non-room session instead gets the engine REST room-control tools.
    const customTools = inRoom
      ? [
          createPostMessageTool(async (text, to) => {
            postedThisTurn = true;
            return emitRoomCommand({ kind: 'message_out', text, to });
          }),
          createInviteAgentTool((spec) => emitRoomCommand({ kind: 'invite', ...spec })),
          ...(isRoomLead
            ? [createCloseRoomTool((spec) => emitRoomCommand({ kind: 'close_room', ...spec }))]
            : []),
        ]
      : fleetEnabled
        ? [
            createOpenRoomTool(),
            createPostRoomTool(),
            createRoomsStatusTool(),
            createFleetCloseRoomTool(),
          ]
        : undefined;
    // Skills: an explicit room capability profile (KILD_SKILLS_PROFILE) is EXCLUSIVE — it
    // replaces pi's defaults with just that dir. Otherwise every session gets pi's defaults
    // PLUS the config-declared skill dirs, so an invited agent can load `prp-implement` when
    // the orchestrator tells it to. This is what makes a plugged-in framework's process
    // reachable by whoever gets invited, not only the lead.
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
    ({ session } = await createAgentSession({
      model,
      authStorage,
      modelRegistry: registry,
      cwd,
      customTools,
      resourceLoader,
    }));
  } catch (err) {
    emit({ kind: 'error', message: errText(err) });
    process.exit(1);
  }
  if (model) emit({ kind: 'model', provider: model.provider, id: model.id });

  session.subscribe((e: RawAgentEvent) => {
    const ui = translate(e);
    if (ui) {
      emit(ui);
      if (ui.kind === 'text') turnText += ui.delta; // accumulate the turn's reply text
    }
    if (e.type === 'agent_end') {
      // Implicit reply: if the agent didn't post explicitly this turn, surface its
      // turn-final text so the human sees what it said. Tagged with the sender for
      // display only — the engine broadcasts it but does NOT deliver it as a turn
      // (see room-router.ts), so narration can't ping-pong agents into a loop.
      if (inRoom && !postedThisTurn && turnText.trim()) {
        process.stdout.write(
          `${JSON.stringify({ kind: 'message_out', text: turnText, to: [turnSender], implicit: true })}\n`,
        );
      }
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
  // Every session gets the generic mechanism guide (how to operate) on top of everything,
  // above the persona — so even a bare `default` session is competent. One-shot: it rides
  // only the first delivered turn. The room-comms part is conditional inside the prompt.
  // A delegating session (room or fleet) also gets the configured model catalog so it can
  // pick a model per fan-out agent.
  const modelsSection =
    inRoom || fleetEnabled ? formatModelsSection(await configuredModels(cwd)) : '';
  let sessionPrefix: string | null = modelsSection
    ? `${MECHANISM_PROMPT}\n\n${modelsSection}`
    : MECHANISM_PROMPT;

  // pi runs one turn at a time. A room can deliver a message (prompt) to this
  // participant while it is still mid-turn, so we queue prompts and drain them
  // strictly sequentially rather than awaiting inside the stdin handler.
  const promptQueue: Array<{ text: string; from: string }> = [];
  let draining = false;
  async function drainPrompts(): Promise<void> {
    if (draining) return;
    draining = true;
    while (promptQueue.length > 0) {
      const next = promptQueue.shift() as { text: string; from: string };
      turnSender = next.from;
      turnText = '';
      postedThisTurn = false;
      try {
        await session.prompt(next.text);
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
        from?: string;
        requestId?: string;
        result?: RoomCommandAck['result'];
      };
      try {
        msg = JSON.parse(line) as typeof msg;
      } catch {
        continue; // a malformed command line must not crash the worker; skip it
      }
      if (msg.type === 'prompt' && msg.text) {
        // First delivered turn carries the preamble: the mechanism guide (how to operate)
        // on top of the persona (<role>). Both ride only the first turn.
        const text = composeSessionTurn(withRole(msg.text, preamble), sessionPrefix);
        promptQueue.push({ text, from: msg.from ?? 'human' });
        preamble = null;
        sessionPrefix = null;
        void drainPrompts();
      } else if (msg.type === 'stop') {
        for (const pending of pendingCommands.values()) pending.reject(new Error('worker stopped'));
        pendingCommands.clear();
        session.dispose();
        process.exit(0);
      } else if (msg.type === 'room_command_result' && msg.requestId && msg.result) {
        const pending = pendingCommands.get(msg.requestId);
        if (!pending) continue;
        pendingCommands.delete(msg.requestId);
        if (msg.result.ok) pending.resolve(msg.result.value.message);
        else pending.reject(new Error(msg.result.message));
      }
    }
  });
  process.stdin.on('end', () => {
    for (const pending of pendingCommands.values()) pending.reject(new Error('worker stdin ended'));
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
