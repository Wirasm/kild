import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * The agent-facing side of a kild: a `send` tool an agent registers (only when it runs
 * in a kild). Calling it is the ONLY way an agent's words reach the other agents or the
 * human — we never scrape the agent's prose, which is the failure mode of pi's coms-net
 * POC. The transport is injected: the agent process passes an `emit` that writes a `send`
 * control line to the engine.
 *
 * Addressing is a structured `to` list, NOT parsed from the message text — so a
 * message body can contain `@decorator` / `@media` / an email without misrouting, and
 * a non-Claude model that forgets sigil syntax still delivers (the engine defaults an
 * omitted `to` to the kild lead). Reaching another agent (delivering them a turn)
 * requires calling this tool — an agent's ordinary prose is never sent for it.
 */
export function createSendTool(
  emit: (text: string, to?: string[]) => Promise<string>,
): ToolDefinition {
  return {
    name: 'send',
    label: 'Send',
    description:
      'Send a message into the kild so other agents and the human can read it. ' +
      'Set `to` to the handles you are addressing (e.g. `["coder"]`, or `["human"]` ' +
      'for the operator) — those agents are prompted with your message. Omit `to` ' +
      'to address the kild lead by default. This is the ONLY way others see your ' +
      'message — your normal output is private to you.',
    promptSnippet: 'send — speak in the kild; set `to` to the handles you address',
    parameters: Type.Object({
      text: Type.String({ description: 'The message body.' }),
      to: Type.Optional(
        Type.Array(Type.String(), {
          description:
            'Handles to address, e.g. ["coder"] or ["human"]. Those agents are ' +
            'prompted with this message. Omit to address the kild lead.',
        }),
      ),
    }),
    async execute(_toolCallId, params) {
      const { text, to } = params as { text: string; to?: string[] };
      const message = await emit(text, to);
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
