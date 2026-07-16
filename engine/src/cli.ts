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
import { resolveModel, withRole } from './kild/models.ts';
import { addProject, findProject, loadProjects, removeProject } from './kild/projects.ts';
import { parseMentions } from './kild/room/parse-mentions.ts';
import {
  ensureWorktree,
  listWorktrees,
  pruneMergedWorktrees,
  removeWorktree,
  type Worktree,
  worktreePath,
} from './kild/worktree.ts';

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    json: { type: 'boolean', default: false },
    project: { type: 'string' },
    agent: { type: 'string' },
    model: { type: 'string' },
    worktree: { type: 'string' },
    participants: { type: 'string' }, // `kild room` participants, e.g. orchestrator,worker,reviewer
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
    case 'worktree':
      return worktree(action, rest);
    case 'run':
      return run([action, ...rest].filter(Boolean).join(' '));
    case 'room':
      return room([action, ...rest].filter(Boolean).join(' '));
    default:
      console.error('usage: kild <project|agent|worktree|run|room> …');
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

// REST helper for the worktree group when routing through a live engine. Surfaces
// the engine's error body (e.g. a 409 when a worktree is in use by a live session).
async function engineFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(`${ENGINE}${path}`, init);
  if (!r.ok) {
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `${path} failed (${r.status})`);
  }
  return r.json() as Promise<T>;
}

async function worktree(action: string | undefined, args: string[]): Promise<void> {
  const repo = values.project
    ? ((await findProject(values.project))?.path ?? values.project)
    : undefined;
  if (!repo) throw new Error('--project <name|path> is required');
  const q = `project=${encodeURIComponent(repo)}`;

  // When the engine is up it owns the live sessions, so route mutations through it —
  // its endpoints skip worktrees a running session is using. When it's down, no live
  // session can exist, so operating directly is safe (and `ls` is read-only).
  const engineUp = await fetch(`${ENGINE}/api/health`)
    .then((r) => r.ok)
    .catch(() => false);

  if (action === 'ls') {
    const trees = engineUp
      ? await engineFetch<Worktree[]>(`/api/worktrees?${q}`)
      : (await listWorktrees(repo)).filter((t) => t.branch.startsWith('kild/'));
    if (json) return void console.log(JSON.stringify(trees, null, 2));
    if (trees.length === 0) return void console.error('no kild worktrees');
    for (const t of trees) console.log(`${t.branch}\t${t.path}`);
  } else if (action === 'rm') {
    const [name] = args;
    if (!name) throw new Error('usage: kild worktree rm <name> --project <p>');
    if (engineUp) {
      await engineFetch(`/api/worktrees`, {
        method: 'DELETE',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ project: repo, name }),
      });
    } else {
      await removeWorktree(repo, worktreePath(name));
    }
    if (json) console.log(JSON.stringify({ ok: true, name }, null, 2));
    else console.log(`removed worktree ${name}`);
  } else if (action === 'prune') {
    const pruned = engineUp
      ? (
          await engineFetch<{ pruned: string[] }>(`/api/worktrees/prune`, {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ project: repo }),
          })
        ).pruned
      : await pruneMergedWorktrees(repo);
    if (json) console.log(JSON.stringify({ pruned }, null, 2));
    else console.log(pruned.length ? `pruned: ${pruned.join(', ')}` : 'nothing to prune');
  } else {
    throw new Error('usage: kild worktree <ls|rm|prune> --project <p>');
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

/**
 * `kild room <goal>` — the room demo. Opens a room of predefined participants
 * (`--participants orchestrator,worker,reviewer`; default `orchestrator,worker`),
 * posts the goal to the first one, and streams every message. With `--worktree
 * <name>` the whole room shares one `kild/<name>` tree (participants attach to it).
 * You can keep typing to post more messages (address participants with @name);
 * Ctrl-C is the kill switch — it closes the room (stopping all participants) and
 * exits. Requires the engine (it is multi-session).
 */
async function room(goal: string): Promise<void> {
  if (!goal) {
    throw new Error(
      'usage: kild room <goal…> [--participants a,b,c] [--worktree <n>] [--project <p>]',
    );
  }
  const engineUp = await fetch(`${ENGINE}/api/health`)
    .then((r) => r.ok)
    .catch(() => false);
  if (!engineUp) {
    throw new Error(`engine not running at ${ENGINE} — start it: cd engine && bun run dev`);
  }
  const cwd = values.project
    ? ((await findProject(values.project))?.path ?? values.project)
    : process.cwd();
  const name = values.project ?? 'room';
  const participantNames = (values.participants ?? 'orchestrator,worker')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  if (participantNames.length === 0) throw new Error('--participants must name at least one agent');
  const lead = participantNames[0] as string;
  // Address the lead unless the goal already addresses a PARTICIPANT. Testing for a
  // bare @mention is not the same question: `@human` is never a participant, and it
  // is exactly what a goal says when it names who to report back to — that would
  // address no one and the room would sit idle.
  const addressed = parseMentions(goal).some((h) => participantNames.includes(h));
  const kickoff = addressed ? goal : `@${lead} ${goal}`;
  const roomId = crypto.randomUUID();
  const ws = new WebSocket(`${ENGINE.replace(/^http/, 'ws')}/ws`);

  await new Promise<void>((resolve, reject) => {
    const closeRoom = () => {
      try {
        ws.send(JSON.stringify({ type: 'room_close', id: roomId }));
      } catch {
        // socket already gone — nothing to close
      }
      setTimeout(() => {
        ws.close();
        resolve();
      }, 200); // let the close frame flush before we exit
    };
    process.on('SIGINT', () => {
      if (!json) console.error('\n\x1b[2m— closing room —\x1b[0m');
      closeRoom();
    });

    // Mid-flight steering: each line you type is posted into the room (address
    // participants with @name). Off in --json mode, which is machine-driven.
    if (!json) {
      process.stdin.setEncoding('utf8');
      process.stdin.on('data', (chunk: string) => {
        for (const raw of chunk.split('\n')) {
          const text = raw.trim();
          if (text) ws.send(JSON.stringify({ type: 'room_post', id: roomId, text }));
        }
      });
    }

    ws.addEventListener('open', () => {
      ws.send(
        JSON.stringify({
          type: 'room_open',
          id: roomId,
          name,
          cwd,
          worktree: values.worktree,
          participants: participantNames.map((n) => ({ name: n, agent: n, model: values.model })),
        }),
      );
      ws.send(JSON.stringify({ type: 'room_post', id: roomId, text: kickoff }));
      if (!json) {
        const where = values.worktree ? ` · tree kild/${values.worktree}` : '';
        console.error(
          `\x1b[2m# room "${name}" — ${participantNames.join(', ')}${where} · type to post · Ctrl-C to stop\x1b[0m`,
        );
      }
    });

    ws.addEventListener('message', (e) => {
      let parsed: {
        roomMessage?: { roomId: string; from: string; to: string[]; text: string };
      };
      try {
        parsed = JSON.parse(String((e as { data: unknown }).data));
      } catch {
        return;
      }
      const m = parsed.roomMessage;
      if (!m || m.roomId !== roomId) return;
      if (json) {
        console.log(JSON.stringify(m));
      } else {
        const to = m.to.length ? ` \x1b[2m→ ${m.to.map((x) => `@${x}`).join(' ')}\x1b[0m` : '';
        console.log(`\x1b[1m${m.from}\x1b[0m${to}: ${m.text}`);
      }
    });

    ws.addEventListener('error', () => reject(new Error('engine socket error')));
  });
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
    // Settle exactly once, then tear the worker down. Without this the CLI hung
    // forever: it only resolved on `agent_end` and only rejected on a socket
    // `error`, so a worker-emitted `error`/`session_end` or a *graceful* close
    // (the engine restarting under `--watch`) left the run waiting on nothing.
    let settled = false;
    const finish = (err?: Error) => {
      if (settled) return;
      settled = true;
      try {
        ws.send(JSON.stringify({ type: 'room_close', id })); // one-shot: tear the room down
      } catch {
        // socket already closing/closed — nothing to close
      }
      ws.close();
      if (err) reject(err);
      else resolve();
    };

    ws.addEventListener('open', () => {
      // A one-shot run is a 1-participant room: the agent is the sole participant, so
      // the bare post (no @mention) is delivered straight to it.
      ws.send(
        JSON.stringify({
          type: 'room_open',
          id,
          name: values.project ?? 'run',
          cwd: projectPath ?? process.cwd(),
          worktree: values.worktree,
          participants: [{ name: 'agent', agent: values.agent, model: values.model }],
        }),
      );
      ws.send(JSON.stringify({ type: 'room_post', id, text: prompt }));
    });
    ws.addEventListener('message', (e) => {
      const msg = JSON.parse(String((e as { data: unknown }).data)) as {
        room?: string;
        participant?: string;
        event?: { kind: string; [k: string]: unknown };
      };
      if (msg.room !== id || !msg.event) return;
      const ev = msg.event;
      if (ev.kind === 'model') model = `${ev.provider}/${ev.id}`;
      else if (ev.kind === 'text') {
        text += ev.delta as string;
        if (!json) process.stdout.write(ev.delta as string);
      } else if (ev.kind === 'tool_start') process.stderr.write(`\x1b[2m🔧 ${ev.name}\x1b[0m\n`);
      else if (ev.kind === 'stats') {
        tokens = ev.tokens as number;
        cost = ev.cost as number;
      } else if (ev.kind === 'error') {
        finish(new Error(String(ev.message ?? 'engine error')));
      } else if (ev.kind === 'agent_end') {
        setTimeout(finish, 150); // settle after the trailing stats event
      } else if (ev.kind === 'session_end') {
        finish(); // worker ended (normally after our stop, or abnormally) — don't hang
      }
    });
    // A graceful close (e.g. the engine reloading under `--watch`) is not an
    // 'error' event, so it needs its own settle path or the run hangs.
    ws.addEventListener('close', () =>
      finish(new Error('connection to the engine closed before the run completed')),
    );
    ws.addEventListener('error', () => finish(new Error('engine socket error')));
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
  let cwd = projectPath ?? process.cwd();
  // Engine-down fallback: replicate the worker's create-or-attach so `--worktree`
  // isolates standalone runs too.
  if (values.worktree) cwd = (await ensureWorktree(cwd, values.worktree)).path;

  const authStorage = AuthStorage.create();
  const registry = ModelRegistry.create(authStorage);
  const model = resolveModel(registry, values.model);

  const { session } = await createAgentSession({
    model,
    authStorage,
    modelRegistry: registry,
    cwd,
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

  const instr = values.agent ? await resolveAgentInstructions(values.agent, projectPath) : null;
  await session.prompt(withRole(prompt, instr));
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
