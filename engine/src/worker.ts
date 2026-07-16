import { AuthStorage, createAgentSession, ModelRegistry } from '@earendil-works/pi-coding-agent';

import { resolveAgentInstructions } from './kild/agents.ts';
import { type RawAgentEvent, translate, type UiEvent } from './kild/events.ts';
import { createFleetCloseRoomTool } from './kild/fleet/close-room-tool.ts';
import { createOpenRoomTool } from './kild/fleet/open-room-tool.ts';
import { createPostRoomTool } from './kild/fleet/post-room-tool.ts';
import { createRoomsStatusTool } from './kild/fleet/rooms-status-tool.ts';
import { resolveModel, withRole } from './kild/models.ts';
import { createCloseRoomTool } from './kild/room/close-room-tool.ts';
import { createInviteAgentTool } from './kild/room/invite-agent-tool.ts';
import { createPostMessageTool } from './kild/room/post-message-tool.ts';
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
  const agentName = process.env.KILD_AGENT || undefined;
  const modelPattern = process.env.KILD_MODEL || undefined;
  const inRoom = !!process.env.KILD_ROOM;
  const isRoomLead = process.env.KILD_ROOM_LEAD === '1';
  const fleetEnabled = process.env.KILD_FLEET === '1';

  const emit = (event: UiEvent) => process.stdout.write(`${JSON.stringify(event)}\n`);
  const emitMessage = (text: string, to?: string[], implicit?: boolean) =>
    process.stdout.write(`${JSON.stringify({ kind: 'message_out', text, to, implicit })}\n`);
  const emitInvite = (spec: { name: string; agent?: string; model?: string }) =>
    process.stdout.write(`${JSON.stringify({ kind: 'invite', ...spec })}\n`);
  const emitCloseRoom = (spec: { reason?: string }) =>
    process.stdout.write(`${JSON.stringify({ kind: 'close_room', ...spec })}\n`);

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
          createPostMessageTool((text) => {
            postedThisTurn = true;
            emitMessage(text);
          }),
          createInviteAgentTool(emitInvite),
          ...(isRoomLead ? [createCloseRoomTool(emitCloseRoom)] : []),
        ]
      : fleetEnabled
        ? [
            createOpenRoomTool(),
            createPostRoomTool(),
            createRoomsStatusTool(),
            createFleetCloseRoomTool(),
          ]
        : undefined;
    ({ session } = await createAgentSession({
      model,
      authStorage,
      modelRegistry: registry,
      cwd,
      customTools,
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
        emitMessage(turnText, [turnSender], true);
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
      let msg: { type: string; text?: string; from?: string };
      try {
        msg = JSON.parse(line) as { type: string; text?: string; from?: string };
      } catch {
        continue; // a malformed command line must not crash the worker; skip it
      }
      if (msg.type === 'prompt' && msg.text) {
        promptQueue.push({ text: withRole(msg.text, preamble), from: msg.from ?? 'human' });
        preamble = null; // the agent preamble rides only the first delivered turn
        void drainPrompts();
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
