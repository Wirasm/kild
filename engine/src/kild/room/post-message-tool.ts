import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * The agent-facing side of a room: a `post_message` tool a worker registers (only
 * when it runs as a room participant). Calling it is the ONLY way a participant's
 * words reach the other participants or the human — we never scrape the agent's
 * prose, which is the failure mode of pi's coms-net POC. The transport is injected:
 * the worker passes an `emit` that writes a `message_out` control line to the engine.
 *
 * Note: an agent's turn-final text is still auto-posted as an *implicit reply* so the
 * human can see what it said (see worker.ts) — but that broadcast never prompts another
 * agent. Reaching another participant (delivering them a turn) requires calling this
 * tool with an `@mention`; otherwise your normal output is private to you.
 */
export function createPostMessageTool(emit: (text: string) => void): ToolDefinition {
  return {
    name: 'post_message',
    label: 'Post Message',
    description:
      'Post a message to the room so other agents and the human can read it. ' +
      'Address recipients by handle: `@name` for an agent (e.g. `@worker`), `@human` ' +
      'for the operator. This is the ONLY way others see your message — your normal ' +
      'output is private to you.',
    promptSnippet: 'post_message — speak in the room; address agents/human with @name',
    parameters: Type.Object({
      text: Type.String({
        description: 'The message body. Mention recipients with @name (@human for the operator).',
      }),
    }),
    async execute(_toolCallId, params) {
      const { text } = params as { text: string };
      emit(text);
      return {
        content: [{ type: 'text' as const, text: 'Posted to the room.' }],
        details: null,
      };
    },
  };
}
