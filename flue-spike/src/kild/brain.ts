import { createAgent, defineTool, Type } from '@flue/runtime';
import type { CreatedAgent, FlueContext, ToolDefinition } from '@flue/runtime';

import { listAgents } from './agents.ts';
import { DEFAULT_MODEL } from './config.ts';
import { createWorktree } from './worktree.ts';
import { findProject, loadProjects } from './projects.ts';
import { rooms } from './rooms.ts';
import { runToCompletion } from './run.ts';

type Init = FlueContext['init'];

/**
 * kild's capabilities, exposed to the brain agent as callable tools — the
 * operator mirror. Because the orchestration layer is in-runtime TypeScript,
 * the agent can *do* what the human operator can by calling these directly.
 * (In the Rust kild, the brain would have to shell out to the kild CLI.)
 */
export function kildTools(init: Init): ToolDefinition[] {
  return [
    defineTool({
      name: 'list_projects',
      description: 'List registered kild projects as a JSON array of {name,path}.',
      parameters: Type.Object({}),
      execute: async () => JSON.stringify(await loadProjects()),
    }),
    defineTool({
      name: 'list_agents',
      description: 'List the agent names available to a project.',
      parameters: Type.Object({ project: Type.String({ description: 'Project name.' }) }),
      execute: async (args) => {
        const project = await findProject(String((args as { project: string }).project));
        const agents = await listAgents(project?.path);
        return JSON.stringify(agents.map((a) => a.name));
      },
    }),
    defineTool({
      name: 'create_worktree',
      description: 'Create an isolated git worktree for a project on a new branch. Returns {branch,path}.',
      parameters: Type.Object({
        project: Type.String({ description: 'Registered project name.' }),
        branch: Type.String({ description: 'Short branch name, e.g. "fix-auth".' }),
      }),
      execute: async (args) => {
        const a = args as { project: string; branch: string };
        const project = await findProject(a.project);
        if (!project) return `error: no such project ${a.project}`;
        return JSON.stringify(await createWorktree(project.path, a.branch));
      },
    }),
    defineTool({
      name: 'run_agent_in_worktree',
      description: "Dispatch a coding agent to do a task inside a worktree path. Returns the agent's reply.",
      parameters: Type.Object({
        worktreePath: Type.String({ description: 'Path returned by create_worktree.' }),
        prompt: Type.String({ description: 'The task for the agent.' }),
      }),
      execute: async (args) => {
        const a = args as { worktreePath: string; prompt: string };
        const outcome = await runToCompletion(init, { prompt: a.prompt, projectPath: a.worktreePath });
        return outcome.text;
      },
    }),
    defineTool({
      name: 'post_to_room',
      description: 'Broadcast a status update to a kild room the human and other agents observe.',
      parameters: Type.Object({
        room: Type.String({ description: 'Room name.' }),
        text: Type.String({ description: 'The update.' }),
      }),
      execute: async (args) => {
        const a = args as { room: string; text: string };
        rooms.post(a.room, 'brain', a.text);
        return 'posted';
      },
    }),
  ];
}

/** The brain: one agent configured with kild's full capability surface. */
export function createBrain(init: Init, model: string = DEFAULT_MODEL): CreatedAgent {
  return createAgent(() => ({
    model,
    instructions:
      'You are the kild operator brain — a mirror of the human operator. You orchestrate ' +
      'coding-agent work across projects using your kild tools: inspect projects and agents, ' +
      'create isolated worktrees, dispatch agents into them, and report progress to rooms. ' +
      'Prefer doing the work via tools over describing it. Be concise.',
    tools: kildTools(init),
  }));
}
