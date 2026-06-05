import { createAgent, type FlueContext } from '@flue/runtime';

import { DEFAULT_MODEL } from '../kild/config.ts';
import { rooms, roomTools } from '../kild/rooms.ts';

/**
 * BATTERY #2 — agent rooms / peer-to-peer comms.
 *
 * Two independent agents (planner, reviewer) — separate harnesses, no
 * parent/child relationship — exchange messages through a kild room via
 * `room_send`/`room_read` tools. This is the thing Flue's `session.task()` tree
 * cannot express, and the kild-owned differentiator.
 */
export async function run({ init }: FlueContext) {
  const room = 'design-review';

  const planner = createAgent(() => ({
    model: DEFAULT_MODEL,
    instructions: 'You are the planner. Use the room tools. Keep each message to one sentence.',
    tools: roomTools(room, 'planner'),
  }));
  const reviewer = createAgent(() => ({
    model: DEFAULT_MODEL,
    instructions: 'You are the reviewer. Use the room tools. Keep each message to one sentence.',
    tools: roomTools(room, 'reviewer'),
  }));

  const plannerSession = await (await init(planner, { name: 'planner' })).session();
  const reviewerSession = await (await init(reviewer, { name: 'reviewer' })).session();

  await plannerSession.prompt('Use room_send to propose: "Add OAuth login to the API." Then stop.');
  await reviewerSession.prompt(
    'Use room_read to see the proposal, then use room_send to reply with exactly one concern. Then stop.',
  );
  await plannerSession.prompt(
    'Use room_read to read the reply, then use room_send to acknowledge it in one sentence. Then stop.',
  );

  return {
    members: rooms.roster(room),
    transcript: rooms.history(room).map((m) => `${m.from}: ${m.text}`),
  };
}
