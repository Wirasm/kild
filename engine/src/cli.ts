#!/usr/bin/env bun
/**
 * kild CLI — the secondary interface. Gives any command-line agent a kild
 * runtime over the Bash tool (see the kild-cli skill), independent of the UI.
 * Thin: parse → call the engine lib → format. Reads → stdout, progress →
 * stderr, non-zero exit on failure.
 */
import { parseArgs } from 'node:util';

import { AuthStorage, createAgentSession, ModelRegistry } from '@earendil-works/pi-coding-agent';

import { listAgents, resolveAgentInstructions } from './kild/agents.ts';
import { addProject, findProject, loadProjects, removeProject } from './kild/projects.ts';

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    json: { type: 'boolean', default: false },
    project: { type: 'string' },
    agent: { type: 'string' },
    model: { type: 'string' },
  },
});

const json = values.json ?? false;
const [group, action, ...rest] = positionals;
const ENGINE = process.env.KILD_ENGINE ?? 'http://localhost:4517';

try {
  await dispatch();
  process.exit(0);
} catch (err) {
  console.error(`\x1b[31merror:\x1b[0m ${err instanceof Error ? err.message : err}`);
  process.exit(1);
}

async function dispatch(): Promise<void> {
  switch (group) {
    case 'project':
      return project(action, rest);
    case 'agent':
      return agent(action, rest);
    case 'run':
      return run([action, ...rest].filter(Boolean).join(' '));
    default:
      console.error('usage: kild <project|agent|run> …');
      process.exit(2);
  }
}

async function project(action: string | undefined, args: string[]): Promise<void> {
  if (action === 'ls') {
    const projects = await loadProjects();
    if (json) return void console.log(JSON.stringify(projects, null, 2));
    if (projects.length === 0) return void console.error('no projects registered');
    for (const p of projects) console.log(`${p.name}\t${p.path}`);
  } else if (action === 'add') {
    const [name, path] = args;
    if (!name || !path) throw new Error('usage: kild project add <name> <path>');
    const p = await addProject(name, path);
    console.log(json ? JSON.stringify(p, null, 2) : `added ${p.name} → ${p.path}`);
  } else if (action === 'rm') {
    const [name] = args;
    if (!name) throw new Error('usage: kild project rm <name>');
    await removeProject(name);
    if (!json) console.log(`removed ${name}`);
  } else {
    throw new Error('usage: kild project <ls|add|rm>');
  }
}

async function agent(action: string | undefined, args: string[]): Promise<void> {
  const projectPath = values.project
    ? ((await findProject(values.project))?.path ?? values.project)
    : undefined;
  if (action === 'ls') {
    const agents = await listAgents(projectPath);
    if (json) return void console.log(JSON.stringify(agents, null, 2));
    for (const a of agents) console.log(a.name);
  } else if (action === 'show') {
    const [name] = args;
    if (!name) throw new Error('usage: kild agent show <name>');
    const found = (await listAgents(projectPath)).find((a) => a.name === name);
    if (!found) throw new Error(`no such agent: ${name}`);
    if (json) console.log(JSON.stringify(found, null, 2));
    else if (found.systemPrompt) console.log(found.systemPrompt);
    else console.error(`(agent '${name}' uses pi's default prompt)`);
  } else {
    throw new Error('usage: kild agent <ls|show>');
  }
}

async function run(prompt: string): Promise<void> {
  if (!prompt) throw new Error('usage: kild run <prompt…>');
  // If the engine is up, run THROUGH it so the session shows up in the cockpit;
  // otherwise run the agent in-process so the CLI works standalone.
  const engineUp = await fetch(`${ENGINE}/api/health`)
    .then((r) => r.ok)
    .catch(() => false);
  return engineUp ? runViaEngine(prompt) : runInProcess(prompt);
}

async function runViaEngine(prompt: string): Promise<void> {
  const projectPath = values.project ? (await findProject(values.project))?.path : undefined;
  const id = crypto.randomUUID();
  const ws = new WebSocket(`${ENGINE.replace(/^http/, 'ws')}/ws`);

  let text = '';
  let model = 'default';
  let tokens = 0;
  let cost = 0;

  await new Promise<void>((resolve, reject) => {
    ws.addEventListener('open', () => {
      ws.send(
        JSON.stringify({
          type: 'spawn',
          id,
          model: values.model,
          cwd: projectPath ?? process.cwd(),
          agent: values.agent,
          projectName: values.project,
          origin: 'cli',
        }),
      );
      ws.send(JSON.stringify({ type: 'prompt', id, text: prompt }));
    });
    ws.addEventListener('message', (e) => {
      const msg = JSON.parse(String((e as { data: unknown }).data)) as {
        session?: string;
        event?: { kind: string; [k: string]: unknown };
      };
      if (msg.session !== id || !msg.event) return;
      const ev = msg.event;
      if (ev.kind === 'model') model = `${ev.provider}/${ev.id}`;
      else if (ev.kind === 'text') {
        text += ev.delta as string;
        if (!json) process.stdout.write(ev.delta as string);
      } else if (ev.kind === 'tool_start') process.stderr.write(`\x1b[2m🔧 ${ev.name}\x1b[0m\n`);
      else if (ev.kind === 'stats') {
        tokens = ev.tokens as number;
        cost = ev.cost as number;
      } else if (ev.kind === 'agent_end') {
        setTimeout(() => {
          ws.send(JSON.stringify({ type: 'stop', id })); // one-shot: clean up the worker
          ws.close();
          resolve();
        }, 150); // catch the trailing stats event
      }
    });
    ws.addEventListener('error', () => reject(new Error('engine socket error')));
  });

  if (json) {
    console.log(JSON.stringify({ model, text, tokens, cost }, null, 2));
  } else {
    if (!text.endsWith('\n')) console.log();
    console.error(
      `\x1b[2m───── ${model}  tokens=${tokens}  cost=$${cost.toFixed(4)}  · live in kild UI\x1b[0m`,
    );
  }
}

async function runInProcess(prompt: string): Promise<void> {
  const projectPath = values.project ? (await findProject(values.project))?.path : undefined;

  const authStorage = AuthStorage.create();
  const registry = ModelRegistry.create(authStorage);
  const model = resolveModel(registry, values.model);

  const { session } = await createAgentSession({
    model,
    authStorage,
    modelRegistry: registry,
    cwd: projectPath ?? process.cwd(),
  });

  let text = '';
  session.subscribe(
    (e: {
      type: string;
      assistantMessageEvent?: { type?: string; delta?: string };
      toolName?: string;
    }) => {
      if (e.type === 'message_update' && e.assistantMessageEvent?.type === 'text_delta') {
        text += e.assistantMessageEvent.delta ?? '';
      } else if (e.type === 'tool_execution_start') {
        process.stderr.write(`\x1b[2m🔧 ${e.toolName}\x1b[0m\n`);
      }
    },
  );

  let message = prompt;
  if (values.agent) {
    const instr = await resolveAgentInstructions(values.agent, projectPath);
    if (instr) message = `<role>\n${instr}\n</role>\n\n${prompt}`;
  }
  await session.prompt(message);
  const stats = session.getSessionStats();
  session.dispose();

  const outcome = {
    model: model ? `${model.provider}/${model.id}` : 'default',
    text,
    tokens: stats.tokens.total,
    cost: stats.cost,
  };
  if (json) {
    console.log(JSON.stringify(outcome, null, 2));
  } else {
    console.log(text);
    console.error(
      `\x1b[2m───── ${outcome.model}  tokens=${outcome.tokens}  cost=$${outcome.cost.toFixed(4)}\x1b[0m`,
    );
  }
}

function resolveModel(registry: ModelRegistry, pattern?: string) {
  if (!pattern) return undefined;
  const slash = pattern.indexOf('/');
  if (slash !== -1) return registry.find(pattern.slice(0, slash), pattern.slice(slash + 1));
  return registry.getAll().find((m) => m.id === pattern);
}
