import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { openRoom } from './engine-client.ts';

export function createOpenRoomTool(): ToolDefinition {
  return {
    name: 'open_room',
    label: 'Open Room',
    description:
      'Open a kild room through the engine REST API. The engine generates the room id, ' +
      'resolves the workspace, posts the kickoff message, and returns the id.',
    promptSnippet: 'open_room — create a room and get back its id',
    parameters: Type.Object({
      name: Type.String({ description: 'Short room name, e.g. "ops".' }),
      cwd: Type.Optional(Type.String({ description: 'Workspace path for the room.' })),
      project: Type.Optional(
        Type.String({ description: 'Registered project name or raw project path.' }),
      ),
      worktree: Type.Optional(Type.String({ description: 'Optional shared worktree name.' })),
      participants: Type.Array(
        Type.Object({
          name: Type.String({ description: 'Participant @handle.' }),
          agent: Type.Optional(Type.String({ description: 'Agent definition to run.' })),
          model: Type.Optional(Type.String({ description: 'Optional model override.' })),
        }),
      ),
      kickoff: Type.String({ description: 'Initial room goal or steering message.' }),
    }),
    async execute(_toolCallId, params) {
      // The fleet brain is the only holder of this tool: attribute its kickoff
      // honestly — the transcript must never claim the human spoke.
      const sessionId = process.env.KILD_SESSION_ID;
      const req = {
        ...(params as Parameters<typeof openRoom>[0]),
        from: 'brain',
        ...(sessionId ? { openedBy: sessionId } : {}),
      };
      const { id, message } = await openRoom(req);
      return {
        content: [{ type: 'text' as const, text: `${message} id=${id}` }],
        details: null,
      };
    },
  };
}
