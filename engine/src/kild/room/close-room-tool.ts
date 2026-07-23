import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * Lets the room's LEAD participant end the room — the teardown counterpart of
 * `invite_agent`. Only the lead registers this (the engine decides; see room-manager).
 * Calling it emits a `close_room` control line the engine turns into: a system notice,
 * every participant session stopped, and the room archived.
 *
 * Closing is DESTRUCTIVE: it kills every agent's in-memory context, which cannot be
 * recovered. So closing is the operator's call, never an autonomous "I think we're done"
 * — the tool description tells the lead to close only on an explicit human instruction. A
 * finished room should idle (agents stay alive, re-promptable with full context) until the
 * human decides the workstream is over.
 */
export function createCloseRoomTool(
  emit: (spec: { reason?: string }) => Promise<string>,
): ToolDefinition {
  return {
    name: 'close_room',
    label: 'Close Room',
    description:
      'Close this room: stops every participant and archives the transcript. This is ' +
      'DESTRUCTIVE — it kills every agent (including you) and their context cannot be ' +
      'recovered. Call it ONLY when the human explicitly tells you to close the room. When ' +
      'your work is done, post your final report to @human and STOP — do NOT close the ' +
      'room; leave it idle so the human can follow up, and let them decide when to close.',
    promptSnippet: 'close_room — end the room ONLY on the human’s explicit instruction',
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
