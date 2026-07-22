import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * The agent-facing side of a room: a `post_message` tool a worker registers (only
 * when it runs as a room participant). Calling it is the ONLY way a participant's
 * words reach the other participants or the human — we never scrape the agent's
 * prose, which is the failure mode of pi's coms-net POC. The transport is injected:
 * the worker passes an `emit` that writes a `message_out` control line to the engine.
 *
 * Addressing is a structured `to` list, NOT parsed from the message text — so a
 * message body can contain `@decorator` / `@media` / an email without misrouting, and
 * a non-Claude model that forgets sigil syntax still delivers (the engine defaults an
 * omitted `to` to the room lead). Reaching another participant (delivering them a turn)
 * requires calling this tool; an agent's turn-final narration is auto-posted as an
 * *implicit reply* so the human sees it, but that broadcast never prompts another agent.
 */
export function createPostMessageTool(
  emit: (text: string, to?: string[]) => Promise<string>,
): ToolDefinition {
  return {
    name: 'post_message',
    label: 'Post Message',
    description:
      'Post a message to the room so other agents and the human can read it. ' +
      'Set `to` to the handles you are addressing (e.g. `["worker"]`, or `["human"]` ' +
      'for the operator) — those participants are prompted with your message. Omit `to` ' +
      'to address the room lead by default. This is the ONLY way others see your ' +
      'message — your normal output is private to you.',
    promptSnippet: 'post_message — speak in the room; set `to` to the handles you address',
    parameters: Type.Object({
      text: Type.String({ description: 'The message body.' }),
      to: Type.Optional(
        Type.Array(Type.String(), {
          description:
            'Handles to address, e.g. ["worker"] or ["human"]. Those participants are ' +
            'prompted with this message. Omit to address the room lead.',
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
