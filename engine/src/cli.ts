#!/usr/bin/env bun
/**
 * kild CLI — the secondary interface. Gives any command-line agent a kild
 * runtime over the Bash tool (see the kild-cli skill), independent of the UI.
 * Thin: parse → call the engine lib → format. Reads → stdout, progress →
 * stderr, non-zero exit on failure.
 */
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { listAgents } from './kild/agents.ts';
import { closeRoom, getLiveRooms, openRoom, postRoom } from './kild/fleet/engine-client.ts';
import { compactLiveRooms } from './kild/fleet/rooms-status.ts';
import { addProject, findProject, loadProjects, removeProject } from './kild/projects.ts';
import {
  forceRemoveWorktree,
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
    force: { type: 'boolean', default: false },
    participants: { type: 'string' }, // `kild room` participants, e.g. orchestrator,worker,reviewer
    detach: { type: 'boolean', default: false }, // `kild room open --detach`: print the id, don't stream
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
      return room(action, rest);
    case 'rooms':
      return roomsList();
    case 'fleet':
      return fleet([action, ...rest].filter(Boolean).join(' '));
    default:
      console.error('usage: kild <project|agent|worktree|run|room|rooms|fleet> …');
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
    const body = (await r.json().catch(() => ({}))) as { error?: string; files?: string[] };
    const preview = body.files?.length ? `: ${body.files.join(', ')}` : '';
    throw new Error(`${body.error ?? `${path} failed (${r.status})`}${preview}`);
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
        body: JSON.stringify({ project: repo, name, force: values.force }),
      });
    } else {
      const result = values.force
        ? await forceRemoveWorktree(repo, worktreePath(name))
        : await removeWorktree(repo, worktreePath(name));
      if (!result.ok) throw new Error(removeRefusalMessage(name, result));
    }
    if (json) console.log(JSON.stringify({ ok: true, name }, null, 2));
    else console.log(`${values.force ? 'force-removed' : 'removed'} worktree ${name}`);
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
    throw new Error('usage: kild worktree <ls|rm|prune> --project <p> [--force]');
  }
}

function removeRefusalMessage(
  name: string,
  refusal: { code: 'dirty' | 'in_use' | 'not_found'; files?: string[] },
): string {
  if (refusal.code === 'dirty') {
    const files = refusal.files?.join(', ') || '(unknown files)';
    return `worktree '${name}' has uncommitted or untracked files: ${files}. Re-run with --force to discard them.`;
  }
  if (refusal.code === 'in_use') return `worktree '${name}' is in use by a live session`;
  return `worktree '${name}' was not found`;
}

async function engineRunning(): Promise<boolean> {
  return fetch(`${ENGINE}/api/health`)
    .then((r) => r.ok)
    .catch(() => false);
}

async function run(prompt: string): Promise<void> {
  if (!prompt) throw new Error('usage: kild run <prompt…>');
  // If the engine is up, run THROUGH it so the session shows up in the cockpit;
  // otherwise run the agent in-process so the CLI works standalone.
  return (await engineRunning()) ? runViaEngine(prompt) : runViaWorker(prompt);
}

async function fleet(goal: string): Promise<void> {
  if (values.worktree) {
    throw new Error('kild fleet does not support --worktree; use kild room or kild run instead');
  }
  if (!goal) throw new Error('usage: kild fleet <goal…> [--project <p>]');
  if (!(await engineRunning())) {
    throw new Error(`engine not running at ${ENGINE} — start it: cd engine && bun run dev`);
  }

  const cwd = values.project
    ? ((await findProject(values.project))?.path ?? values.project)
    : process.cwd();
  const id = crypto.randomUUID();
  const ws = new WebSocket(`${ENGINE.replace(/^http/, 'ws')}/ws`);

  await new Promise<void>((resolve, reject) => {
    let settled = false;
    let stopping = false;
    const finish = (err?: Error) => {
      if (settled) return;
      settled = true;
      ws.close();
      if (err) reject(err);
      else resolve();
    };
    const stop = () => {
      if (stopping) return;
      stopping = true;
      try {
        ws.send(JSON.stringify({ type: 'stop', id }));
      } catch {
        finish();
        return;
      }
      setTimeout(() => finish(), 200);
    };

    process.on('SIGINT', () => {
      if (!json) console.error('\n\x1b[2m— stopping fleet session —\x1b[0m');
      stop();
    });

    if (!json) {
      process.stdin.setEncoding('utf8');
      process.stdin.on('data', (chunk: string) => {
        for (const raw of chunk.split('\n')) {
          const text = raw.trim();
          if (!text) continue;
          ws.send(JSON.stringify({ type: 'prompt', id, text }));
        }
      });
    }

    ws.addEventListener('open', () => {
      ws.send(
        JSON.stringify({
          type: 'spawn',
          id,
          // The fleet driver's persona comes from the project (`--agent <name>`); with none,
          // it's the `default` general-purpose session (kild's system prompt) plus the fleet
          // room-control tools. kild ships no `brain` role of its own.
          agent: values.agent ?? 'default',
          cwd,
          model: values.model,
          worktree: values.worktree,
          projectName: values.project ?? 'fleet',
          env: { KILD_FLEET: '1' },
        }),
      );
      ws.send(JSON.stringify({ type: 'prompt', id, text: goal }));
      if (!json) {
        const persona = values.agent ? ` (${values.agent})` : '';
        const where = values.worktree ? ` · tree kild/${values.worktree}` : '';
        console.error(`\x1b[2m# fleet${persona}${where} · type to prompt · Ctrl-C to stop\x1b[0m`);
      }
    });

    ws.addEventListener('message', (e) => {
      const msg = JSON.parse(String((e as { data: unknown }).data)) as {
        session?: string;
        event?: { kind: string; [k: string]: unknown };
      };
      if (msg.session !== id || !msg.event) return;
      const ev = msg.event;
      if (ev.kind === 'text') {
        if (!json) process.stdout.write(String(ev.delta ?? ''));
      } else if (ev.kind === 'tool_start') {
        process.stderr.write(`\x1b[2m🔧 ${ev.name}\x1b[0m\n`);
      } else if (ev.kind === 'stats') {
        if (!json) {
          process.stderr.write(
            `\x1b[2m───── tokens=${Number(ev.tokens ?? 0)}  cost=$${Number(ev.cost ?? 0).toFixed(4)}\x1b[0m\n`,
          );
        }
      } else if (ev.kind === 'error') {
        finish(new Error(String(ev.message ?? 'engine error')));
      } else if (ev.kind === 'session_end') {
        finish();
      }
    });
    ws.addEventListener('close', () => {
      if (!settled && !stopping) {
        finish(new Error('connection to the engine closed before the fleet session completed'));
      }
    });
    ws.addEventListener('error', () => finish(new Error('engine socket error')));
  });
}

/**
 * `kild room <goal>` — opens a room of participants (`--participants a,b,c`, each a
 * persona from the project's own agents; with none, one general-purpose `default`
 * participant), posts the goal to the lead, and streams every message. With `--worktree
 * <name>` the whole room shares one `kild/<name>` tree (participants attach to it).
 * You can keep typing to post more messages (address participants with @name).
 * The run ends when the room does: the lead closes it with its `close_room` tool
 * after the final report, or Ctrl-C is the kill switch — it closes the room
 * (stopping all participants) and exits. Requires the engine (it is multi-session).
 */
/** Shared spec for opening a room from either the interactive or `--detach` path. */
function roomParticipants(): Array<{ name: string; agent: string; model?: string }> {
  return values.participants
    ? values.participants
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
        .map((n) => ({ name: n, agent: n, model: values.model }))
    : [{ name: 'agent', agent: 'default', model: values.model }];
}

async function roomCwd(): Promise<string> {
  return values.project
    ? ((await findProject(values.project))?.path ?? values.project)
    : process.cwd();
}

/** `kild room <ls|open|post|close>` — the scriptable, non-interactive room primitives an
 *  external driver (agent or human over bash) needs. A bare goal stays interactive. */
async function room(action: string | undefined, args: string[]): Promise<void> {
  if (action === 'ls') return roomsList();
  if (action === 'open') return roomOpen(args.join(' '));
  if (action === 'post') {
    const [id, ...text] = args;
    if (!id || text.length === 0) throw new Error('usage: kild room post <id> <text…>');
    return roomPost(id, text.join(' '));
  }
  if (action === 'close') {
    const [id] = args;
    if (!id) throw new Error('usage: kild room close <id>');
    return roomClose(id);
  }
  return roomInteractive([action, ...args].filter(Boolean).join(' '));
}

/** `kild rooms` / `kild room ls` — live rooms with their code-state observability. */
async function roomsList(): Promise<void> {
  const rooms = compactLiveRooms(await getLiveRooms());
  if (json) return void console.log(JSON.stringify(rooms, null, 2));
  if (rooms.length === 0) return void console.error('no live rooms');
  for (const r of rooms) {
    const parts = r.participants.map((p) => p.name).join(', ');
    const g = r.git;
    const git = g
      ? ` · ${g.branch ?? '?'} +${g.ahead}/-${g.behind}${g.dirty ? ' dirty' : ''}${g.conflictsWithBase ? ' CONFLICTS' : ''}`
      : '';
    const col = r.collidesWith?.length
      ? ` · collides: ${r.collidesWith.map((c) => `${c.room}(${c.files.length})`).join(', ')}`
      : '';
    console.log(`${r.id}\t${r.name} [${parts}]${git}${col}`);
  }
}

/** `kild room open <goal> --detach` — open a room, print its id, return (no streaming). */
async function roomOpen(goal: string): Promise<void> {
  if (!goal) throw new Error('usage: kild room open <goal…> [--participants a,b] [--detach]');
  if (!values.detach) return roomInteractive(goal);
  const res = await openRoom({
    name: values.project ?? 'room',
    cwd: await roomCwd(),
    participants: roomParticipants(),
    worktree: values.worktree,
    kickoff: goal,
  });
  console.log(json ? JSON.stringify(res, null, 2) : res.id);
}

/** `kild room post <id> <text>` — steer an existing room from a separate call. */
async function roomPost(id: string, text: string): Promise<void> {
  const res = await postRoom(id, text);
  if (json) console.log(JSON.stringify(res, null, 2));
  else console.error(res.message);
}

/** `kild room close <id>` — close a specific room by id. */
async function roomClose(id: string): Promise<void> {
  const res = await closeRoom(id);
  if (json) console.log(JSON.stringify(res, null, 2));
  else console.error(res.message);
}

async function roomInteractive(goal: string): Promise<void> {
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
  // `--participants a,b` names personas from the project's own agents; each participant's
  // agent defaults to its name. With none given, kild opens one general-purpose
  // participant (the `default` persona = kild's system prompt, no role) — kild ships no
  // roles of its own; personas come from the project (.claude/agents / .pi/agents).
  const participants = values.participants
    ? values.participants
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
        .map((n) => ({ name: n, agent: n }))
    : [{ name: 'agent', agent: 'default' }];
  if (participants.length === 0) throw new Error('--participants must name at least one agent');
  // Addressing is structured: the engine defaults an untargeted post to the room lead,
  // so the goal reaches the lead without munging the text.
  const kickoff = goal;
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
          if (!text) continue;
          const invite = text.match(/^\/invite(?:\s+(\S+))?(?:\s+(\S+))?(?:\s+(\S+))?$/);
          if (invite?.[1]) {
            const [, name, agent, model] = invite;
            ws.send(
              JSON.stringify({ type: 'room_add', id: roomId, participant: { name, agent, model } }),
            );
          } else {
            ws.send(JSON.stringify({ type: 'room_post', id: roomId, text }));
          }
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
          participants: participants.map((p) => ({ ...p, model: values.model })),
        }),
      );
      ws.send(JSON.stringify({ type: 'room_post', id: roomId, text: kickoff }));
      if (!json) {
        const where = values.worktree ? ` · tree kild/${values.worktree}` : '';
        console.error(
          `\x1b[2m# room "${name}" — ${participants.map((p) => p.name).join(', ')}${where} · type to post · /invite <name> [agent] [model] · Ctrl-C to stop\x1b[0m`,
        );
      }
    });

    ws.addEventListener('message', (e) => {
      let parsed: {
        roomMessage?: { roomId: string; from: string; to: string[]; text: string };
        archivedRoom?: { id: string };
      };
      try {
        parsed = JSON.parse(String((e as { data: unknown }).data));
      } catch {
        return;
      }
      // The engine archived our room (the lead called `close_room`, or another
      // client closed it) — the run is over; resolve without re-closing.
      if (parsed.archivedRoom?.id === roomId) {
        if (!json) console.error('\x1b[2m— room closed —\x1b[0m');
        ws.close();
        resolve();
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

/**
 * Run through the same JSONL worker boundary as the engine. `server.ts` owns the
 * worker role dispatch, so invoke that entry explicitly rather than re-invoking this
 * CLI entry (which would otherwise start a second CLI process under `KILD_ROLE`).
 */
async function runViaWorker(prompt: string): Promise<void> {
  const projectPath = values.project ? (await findProject(values.project))?.path : undefined;
  const workerEntry = fileURLToPath(new URL('./server.ts', import.meta.url));
  const child = spawn(process.execPath, [workerEntry], {
    env: {
      ...process.env,
      KILD_ROLE: 'worker',
      KILD_CWD: projectPath ?? process.cwd(),
      KILD_AGENT: values.agent ?? '',
      KILD_MODEL: values.model ?? '',
      KILD_WORKTREE: values.worktree ?? '',
    },
    stdio: ['pipe', 'pipe', 'inherit'],
  });

  let text = '';
  let model = 'default';
  let tokens = 0;
  let cost = 0;
  let buffer = '';
  let agentEnded = false;

  await new Promise<void>((resolve, reject) => {
    let settled = false;
    const finish = (err?: Error) => {
      if (settled) return;
      settled = true;
      if (err) reject(err);
      else resolve();
    };
    const stop = () => {
      child.stdin?.end(`${JSON.stringify({ type: 'stop' })}\n`);
    };
    const consume = (line: string) => {
      let event: { kind?: string; [key: string]: unknown };
      try {
        event = JSON.parse(line) as { kind?: string; [key: string]: unknown };
      } catch {
        finish(new Error(`worker emitted invalid JSONL: ${line}`));
        return;
      }
      switch (event.kind) {
        case 'model':
          model = `${event.provider}/${event.id}`;
          break;
        case 'text': {
          const delta = String(event.delta ?? '');
          text += delta;
          if (!json) process.stdout.write(delta);
          break;
        }
        case 'tool_start':
          process.stderr.write(`\x1b[2m🔧 ${event.name}\x1b[0m\n`);
          break;
        case 'stats':
          tokens = Number(event.tokens ?? 0);
          cost = Number(event.cost ?? 0);
          break;
        case 'error':
          finish(new Error(String(event.message ?? 'worker error')));
          break;
        case 'agent_end':
          agentEnded = true;
          // The worker writes stats immediately after agent_end. Defer stopping until
          // this stdout batch has been consumed so the final outcome includes them.
          setTimeout(stop, 0);
          break;
      }
    };

    child.stdout?.on('data', (chunk: Buffer) => {
      buffer += chunk.toString();
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const raw of lines) {
        const line = raw.trim();
        if (line) consume(line);
      }
    });
    child.on('error', (err) => finish(new Error(`worker failed: ${err.message}`)));
    child.on('close', (code) => {
      if (buffer.trim()) consume(buffer.trim());
      if (!agentEnded)
        finish(new Error(`worker exited before completing (code ${code ?? 'unknown'})`));
      else finish();
    });
    child.stdin?.write(`${JSON.stringify({ type: 'prompt', text: prompt })}\n`);
  });

  const outcome = { model, text, tokens, cost };
  if (json) {
    console.log(JSON.stringify(outcome, null, 2));
  } else {
    if (!text.endsWith('\n')) console.log();
    console.error(`\x1b[2m───── ${model}  tokens=${tokens}  cost=$${cost.toFixed(4)}\x1b[0m`);
  }
}
