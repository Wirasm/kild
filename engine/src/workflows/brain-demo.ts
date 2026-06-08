import { randomUUID } from 'node:crypto';
import type { FlueContext } from '@flue/runtime';

import { createBrain } from '../kild/brain.ts';
import { startCockpitLog } from '../kild/observability.ts';
import { roomManager } from '../kild/room/room-manager.ts';

/**
 * BATTERY #3 — the brain (operator mirror) + observability.
 *
 * The brain is an agent whose tools ARE kild's capabilities. Give it a goal and
 * it acts by calling them (list_projects, create_worktree, run_agent_in_worktree,
 * open_room, post_to_room) — exactly what the human operator can do, over the same
 * real Room primitive the cockpit renders. The cockpit log shows Flue's event
 * stream powering observability. This is the argument for putting the orchestration
 * layer in-runtime (TS), since the brain calls it directly.
 */
export async function run({ init, payload }: FlueContext) {
  const cockpit = startCockpitLog();
  const p = (payload ?? {}) as { goal?: string };

  // Pre-open a real room so the read-back is deterministic; the brain posts into it.
  const roomId = randomUUID();
  roomManager.open(roomId, { name: 'ops', cwd: process.cwd(), participants: [] });

  const brain = createBrain(init);
  const session = await (await init(brain)).session();

  const goal =
    p.goal ??
    `List the registered projects, then post a one-line status to room "ops" (roomId ${roomId}) ` +
      'stating how many projects there are.';
  const res = await session.prompt(goal);

  return {
    reply: res.text,
    roomOps: roomManager.messages(roomId).map((m) => `${m.from}: ${m.text}`),
    toolCalls: cockpit.filter((e) => e.t === 'tool_start').map((e) => e.detail),
    cockpitEventCount: cockpit.length,
  };
}
