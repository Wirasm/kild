import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * Lets the room's LEAD participant end the room as an explicit act — the teardown
 * counterpart of `invite_agent`. Only the lead registers this (the engine decides;
 * see room-manager). Calling it emits a `close_room` control line the engine turns
 * into: a system notice, every participant session stopped, and the room archived.
 * Without it a finished room idles forever — "the goal is done" is a judgement only
 * an agent (or the operator) can make, so it must be a tool call, not a heuristic.
 */
export function createCloseRoomTool(
  emit: (spec: { reason?: string }) => Promise<string>,
): ToolDefinition {
  return {
    name: 'close_room',
    label: 'Close Room',
    description:
      'Close this room: stops every participant and archives the transcript. Call this ' +
      'ONLY as your very last act, after posting the final report to @human — nothing ' +
      'in the room runs afterwards, including you.',
    promptSnippet: 'close_room — end the room after the final report is posted',
    parameters: Type.Object({
      reason: Type.Optional(
        Type.String({ description: 'One line for the log, e.g. "goal complete".' }),
      ),
    }),
    async execute(_toolCallId, params) {
      const { reason } = params as { reason?: string };
      const message = await emit({ reason });
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
