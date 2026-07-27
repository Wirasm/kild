import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * Lets the kild's LEAD agent end the kild — the teardown counterpart of `spawn`. Only
 * the lead registers this (the engine decides; see kild-manager). Calling it emits a
 * `stop` control line the engine turns into: a system notice, every agent session
 * stopped, and the kild archived.
 *
 * Stopping is DESTRUCTIVE: it kills every agent's in-memory context, which cannot be
 * recovered. So stopping is the operator's call, never an autonomous "I think we're done"
 * — the tool description tells the lead to stop only on an explicit human instruction. A
 * finished kild should idle (agents stay alive, re-promptable with full context) until the
 * human decides the kild's work is over.
 */
export function createStopTool(
  emit: (spec: { reason?: string }) => Promise<string>,
): ToolDefinition {
  return {
    name: 'stop',
    label: 'Stop',
    description:
      'Stop this kild: stops every agent and archives the transcript. This is ' +
      'DESTRUCTIVE — it kills every agent (including you) and their context cannot be ' +
      'recovered. Call it ONLY when the human explicitly tells you to stop the kild. When ' +
      'your work is done, send your final report to @human and STOP — do NOT stop the ' +
      'kild; leave it idle so the human can follow up, and let them decide when to stop it.',
    promptSnippet: 'stop — end the kild ONLY on the human’s explicit instruction',
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
