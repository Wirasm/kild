#!/usr/bin/env bun
/**
 * kild CLI — the secondary interface. Gives any command-line agent a kild
 * runtime over the Bash tool (see the kild-cli skill), independent of the UI.
 * Thin: parse → call the engine lib → format. Reads → stdout, progress →
 * stderr, non-zero exit on failure.
 */
import { spawn } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { findAttachment, recordAttachment } from './kild/attachment.ts';
import { claudeStopOutput } from './kild/claude-stop.ts';
import {
  attachAgent,
  disposeKild,
  drainInbox,
  EngineTimeout,
  getKild,
  kildMessages,
  kildsStatus,
  type LandResponse,
  landKild,
  landPreview,
  listAgents,
  listKilds,
  newKild,
  sendMessage,
  spawnKildAgent,
  stopKildAgent,
  stopKild as stopKildRequest,
} from './kild/engine-client.ts';
import { compactLiveKilds, formatCompactGitSummary } from './kild/kilds-status.ts';
import { GENERAL_PERSONA, listPersonas } from './kild/personas.ts';
import { addProject, findProject, loadProjects, removeProject } from './kild/projects.ts';
import {
  pollResult,
  WATCH_DEFAULT_TIMEOUT_S,
  WATCH_EXIT,
  WATCH_POLL_MS,
  WATCH_TOLERATED_FAILURES,
  watchRequestTimeout,
  watchSummary,
} from './kild/watch.ts';

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    json: { type: 'boolean', default: false },
    project: { type: 'string' },
    persona: { type: 'string' },
    model: { type: 'string' },
    worktree: { type: 'string' },
    force: { type: 'boolean', default: false },
    agents: { type: 'string' }, // `kild new` agents, e.g. orchestrator,coder,reviewer
    detach: { type: 'boolean', default: false }, // `kild new --detach`: print the id, don't stream
    base: { type: 'string' }, // base branch for the worktree + git-status baseline
    as: { type: 'string' }, // `kild attach|inbox --as <handle>`: the attached agent's handle
    format: { type: 'string' }, // `kild inbox --format claude-stop`: harness hook output
    to: { type: 'string' }, // `kild send --to a,b`: the agents addressed
    state: { type: 'string' }, // `kild ls --state live|orphan|reclaimable`
    git: { type: 'boolean', default: false }, // `kild ls --git`: pay for git/cost state
    since: { type: 'string' }, // `kild log --since <seq>`: only messages after that cursor
    execute: { type: 'boolean', default: false }, // `kild land --execute`: merge for real
    task: { type: 'string' }, // `kild spawn --task <text>`: the new agent's first message
    session: { type: 'string' }, // `kild attach|inbox|send --session <id>`: the harness session
    timeout: { type: 'string' }, // `kild watch --timeout <seconds>`: how long to wait
    interval: { type: 'string' }, // `kild watch --interval <seconds>`: wake latency vs load
  },
});

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

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
    case 'persona':
      return persona(action, rest);
    case 'run':
      return run([action, ...rest].filter(Boolean).join(' '));
    case 'ls':
      return kildList();
    case 'new':
      return kildNew([action, ...rest].filter(Boolean).join(' '));
    case 'send': {
      if (!action || rest.length === 0) {
        throw new Error('usage: kild send <id> --to a,b <text…>');
      }
      return kildSend(action, rest.join(' '));
    }
    case 'stop': {
      if (!action) throw new Error('usage: kild stop <id> [--as <handle>]');
      return kildStop(action);
    }
    case 'spawn': {
      if (!action || !values.as) {
        throw new Error(
          'usage: kild spawn <id> --as <handle> [--persona p] [--model m] [--task <text>]',
        );
      }
      return kildSpawn(action, values.as);
    }
    case 'rm': {
      if (!action) throw new Error('usage: kild rm <id|worktree-name> [--force]');
      return kildRm(action);
    }
    case 'land': {
      if (!action) throw new Error('usage: kild land <id|worktree-name> [--execute]');
      return kildLand(action);
    }
    case 'attach':
      return kildAttach(action);
    case 'inbox':
      return kildInbox(action);
    case 'watch':
      return kildWatch(action);
    case 'log': {
      if (!action) throw new Error('usage: kild log <id>');
      return kildLog(action);
    }
    case 'show': {
      if (!action) throw new Error('usage: kild show <id>');
      return kildShow(action);
    }
    case 'agents':
      return agentsList();
    default:
      console.error(
        'usage: kild <ls|new|send|spawn|stop|rm|land|attach|inbox|watch|log|show|agents|persona|project|run> …',
      );
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

/** `--project` is a name-or-path union: a registered project's name, or a path to a
 *  directory. Resolve the name first, then fall back to treating the value as a path —
 *  but only if that path actually exists.
 *
 *  Without the existence check a bare unregistered NAME resolves against the cwd, so
 *  `--project helm` run from `…/sild/helm` became `…/sild/helm/helm`. Nothing downstream
 *  verified it, so the kild was created, its worktree could not be created, no agent ever
 *  spawned, and the command still exited 0 with a kild id. A delegation that silently
 *  does nothing is worse than one that fails. */
async function resolveProjectFlag(): Promise<string | undefined> {
  if (!values.project) return undefined;
  const registered = await findProject(values.project);
  if (registered) return registered.path;

  const asPath = path.resolve(values.project);
  if (existsSync(asPath) && statSync(asPath).isDirectory()) return asPath;

  const known = (await loadProjects()).map((p) => p.name);
  throw new Error(
    `unknown project: ${values.project} — not registered, and ${asPath} is not a directory.` +
      (known.length ? ` Registered: ${known.join(', ')}.` : '') +
      ' Pass a path, or register it with `kild project add <name> <path>`.',
  );
}

/** `kild persona <ls|show>` — the reusable persona definitions the project ships. */
async function persona(action: string | undefined, args: string[]): Promise<void> {
  const projectPath = await resolveProjectFlag();
  if (action === 'ls') {
    const personas = await listPersonas(projectPath);
    if (json) return void console.log(JSON.stringify(personas, null, 2));
    for (const p of personas) console.log(p.name);
  } else if (action === 'show') {
    const [name] = args;
    if (!name) throw new Error('usage: kild persona show <name>');
    const found = (await listPersonas(projectPath)).find((p) => p.name === name);
    if (!found) throw new Error(`no such persona: ${name}`);
    if (json) console.log(JSON.stringify(found, null, 2));
    else if (found.systemPrompt) console.log(found.systemPrompt);
    else console.error(`(persona '${name}' uses pi's default prompt)`);
  } else {
    throw new Error('usage: kild persona <ls|show>');
  }
}

async function engineRunning(): Promise<boolean> {
  return fetch(`${ENGINE}/api/health`)
    .then((r) => r.ok)
    .catch(() => false);
}

async function run(prompt: string): Promise<void> {
  if (!prompt) throw new Error('usage: kild run <prompt…>');
  // If the engine is up, run THROUGH it so the session shows up in UI clients;
  // otherwise run the agent in-process so the CLI works standalone.
  return (await engineRunning()) ? runViaEngine(prompt) : runViaAgent(prompt);
}

/** `kild agents` — list live agents. */
async function agentsList(): Promise<void> {
  const agents = await listAgents();
  if (json) return void console.log(JSON.stringify(agents, null, 2));
  if (agents.length === 0) return void console.error('no live agents');
  for (const a of agents) {
    console.log(`${a.id}\t${a.persona ?? GENERAL_PERSONA}${a.model ? ` (${a.model})` : ''}`);
  }
}

/**
 * `kild new <goal>` — opens a kild of agents (`--agents a,b,c`, each a
 * persona from the project's own personas; with none, one `general` agent), sends the goal
 * to the agents named by `--to`, and streams every message. With
 * `--worktree <name>` the whole kild shares one `kild/<name>` tree (agents attach to it).
 * You can keep typing to send more messages to the same recipients.
 * The run ends when the kild does: an agent stops it with its `stop` tool
 * after the final report, or Ctrl-C is the kill switch — it stops the kild
 * (stopping all agents) and exits. Requires the engine (it is multi-session).
 */
/** Shared spec for opening a kild from either the interactive or `--detach` path. */
function kildAgents(): Array<{ handle: string; persona: string; model?: string }> {
  return values.agents
    ? values.agents
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
        .map((n) => ({ handle: n, persona: n, model: values.model }))
    : [{ handle: 'agent', persona: GENERAL_PERSONA, model: values.model }];
}

async function kildCwd(): Promise<string> {
  return (await resolveProjectFlag()) ?? process.cwd();
}

/**
 * The harness session this invocation belongs to, when it belongs to one.
 *
 * `--session` wins so a harness that knows its own id can say so outright; otherwise take
 * the one Claude Code exports to the tools it runs. A plain human's shell has neither and
 * is simply not a harness session, which is correct.
 *
 * A `function` declaration, not a `const` arrow: `dispatch()` runs at the top of this
 * module, so anything it reaches has to be hoisted or it is still in the temporal dead zone
 * when the first command calls it.
 */
function harnessSession(): string | undefined {
  return values.session ?? process.env.CLAUDE_CODE_SESSION_ID ?? undefined;
}

/**
 * The (kild, handle) this invocation acts as, resolved field by field:
 * **an explicit argument, then this session's recorded attachment, then the environment.**
 *
 * The record outranks the environment, which is the opposite of what it looks like it should
 * be, and the reason is worth keeping: an environment variable here is ambient configuration
 * that nobody re-reads, while a record exists only because THIS session ran `kild attach` —
 * a deliberate, recent act naming a kild that existed at the time. When the two disagree, the
 * deliberate act is the truer one.
 *
 * It is also the only ordering that survives the trap this was found in. A harness's settings
 * file can carry a kild id, that file is re-read on every start including resumes, and it
 * outranks anything a shell exports — so a stale entry pointing at an ARCHIVED kild is
 * invisible, unkillable from the shell, and drains nothing forever while reporting success.
 * Ranking it below an actual attach means a real attach always wins, and the stale value can
 * only ever be a fallback for a session that never attached at all.
 *
 * Pinning up front still works, which was the point of keeping the environment at all. Fields
 * resolve independently, so a hook that passes `--as` and lets the id resolve works too.
 */
async function resolveAttachment(
  id: string | undefined,
  handle: string | undefined,
): Promise<{ kildId?: string; handle?: string }> {
  if (id && handle) return { kildId: id, handle };
  const session = harnessSession();
  // Deliberately NOT guarded. An unreadable record means "there may be an attachment and I
  // cannot read it", and a `kild send` that swallowed that would post the message with no
  // credential — silently unattributed, which is the exact bug this file exists to fix. The
  // one caller allowed to degrade to silence is the turn-end hook, and it does so at its own
  // call site where it knows it is a hook.
  const record = session ? await findAttachment(session) : null;
  return {
    kildId: id ?? record?.kildId ?? process.env.KILD_KILD_ID ?? undefined,
    handle: handle ?? record?.handle ?? process.env.KILD_HANDLE ?? undefined,
  };
}

/** `kild attach <id> --as <handle>` — register an ATTACHED agent: a harness kild
 *  does not own (the Claude Code session you are driving) claiming a `@handle` in the
 *  kild. Nothing is spawned; the handle is addressable immediately. Idempotent.
 *
 *  The attachment is also recorded against this session's own id, which is what lets the
 *  session find its handle again later without carrying it in the environment. That is the
 *  whole of what makes attaching mid-session work: a process's environment is fixed at exec
 *  time, so a session opened before the kild existed can never be told about it any other
 *  way. The token is deliberately NOT recorded — it dies with the engine, so a copy on disk
 *  would go stale and be rejected; `send` mints a fresh one from the idempotent attach. */
async function kildAttach(id: string | undefined): Promise<void> {
  const handle = values.as;
  if (!id || !handle) throw new Error('usage: kild attach <id> --as <handle>');
  const result = await attachAgent(id, handle);
  const session = harnessSession();
  if (session) await recordAttachment(session, id, handle);
  console.log(json ? JSON.stringify(result, null, 2) : result.message);
}

/**
 * `kild inbox <id> --as <handle> [--format claude-stop]` — destructively read that
 * agent's inbox. An empty drain is also its idle signal; there is no status verb.
 *
 * With `--format claude-stop` this IS a Claude Code Stop hook's entire body, so it obeys
 * the hook contract absolutely: print the block JSON when mail is waiting, print **nothing
 * at all** when it is not, and **exit 0 either way** — including when the engine is down,
 * the kild is gone, or the handle was never attached. A hook that cannot reach kild must
 * never stop the operator from finishing a turn. Without the flag it is an ordinary CLI
 * verb and failures are ordinary loud errors.
 */
/**
 * `kild watch [<id> --as <handle>] [--since <seq>] [--timeout <seconds>]` — block until
 * somebody else speaks in the kild, then exit.
 *
 * The wake path a turn-end hook cannot provide: a hook fires when a turn ends, so an idle
 * harness is unreachable until its operator happens to prompt it. Run this in whatever
 * background facility the harness has and react to its exit.
 *
 * It reads the LOG, never the inbox, so it is non-destructive and cannot eat what the Stop
 * hook is there to deliver. Exit codes carry the outcome ({@link WATCH_EXIT}) — in particular
 * a quiet engine and a dead one are different codes, so a harness built on this cannot inherit
 * the silent-failure shape it exists to remove.
 */
async function kildWatch(idArg: string | undefined): Promise<never> {
  const { kildId: id, handle } = await resolveAttachment(idArg, values.as);
  if (!id || !handle) {
    throw new Error(
      'usage: kild watch [<id> --as <handle>] [--since <seq>] [--timeout <s>] [--interval <s>]\n' +
        '  omit id/--as to use the kild this session attached to\n' +
        '  waits for somebody else to speak, then exits: 0 mail, 2 quiet, 3 engine unreachable\n' +
        '  NON-DESTRUCTIVE — it reads the log and consumes nothing. Your mail is still\n' +
        '  waiting afterwards; drain it with `kild inbox`.',
    );
  }
  const timeout = values.timeout === undefined ? WATCH_DEFAULT_TIMEOUT_S : Number(values.timeout);
  if (!Number.isFinite(timeout) || timeout <= 0) {
    throw new Error(`--timeout must be a positive number of seconds (got ${values.timeout})`);
  }

  // Default to the log's current end, so a watcher started now waits for what happens NEXT
  // rather than firing immediately on the conversation it was started in the middle of.
  //
  // This first call needs the same unreachable handling as the poll loop. Left bare it threw
  // to the top-level catch and exited 1 — a *usage* code — so a harness starting a watcher
  // against a dead engine could not tell "the engine is gone" from "you typed it wrong". That
  // is the distinction the exit codes exist for, defeated at the one moment it matters most.
  // Narrowing does not survive into a closure, so pin it once the guard above has run.
  const kildId = id;
  // Wake latency, and the gap between tolerated retries. Tunable because a harness that
  // wants a snappier wake should not have to fork the CLI to get one.
  const interval = values.interval === undefined ? WATCH_POLL_MS : Number(values.interval) * 1000;
  if (!Number.isFinite(interval) || interval <= 0) {
    throw new Error(`--interval must be a positive number of seconds (got ${values.interval})`);
  }
  const started = Date.now();
  const deadline = started + timeout * 1000;
  function sleep(): Promise<unknown> {
    return new Promise((resolve) => setTimeout(resolve, interval));
  }
  let failures = 0;
  /** Whether the engine has answered even once. "Quiet" is a claim ABOUT the engine — that it
   *  responded and had nothing — so it cannot be reported by a watcher that never heard from
   *  it. Without this, a window that closed after nothing but failures exited 2, asserting the
   *  opposite of what was observed. */
  let answered = false;

  /**
   * Ask once. Returns the batch, or null when the failure was tolerated and the caller should
   * wait and try again; exits `unreachable` once tolerance runs out.
   *
   * ONE place that knows how much transient failure is acceptable, used by both the bootstrap
   * fetch and the poll loop. They had separate copies of this decision and immediately drifted:
   * the loop retried with no delay at all, so three "tolerated" failures burned in about eight
   * milliseconds and a merely-restarting engine was declared dead — the exact opposite of what
   * the tolerance exists for — while the bootstrap fetch tolerated nothing whatsoever.
   */
  async function poll(
    since: number | undefined,
  ): Promise<Awaited<ReturnType<typeof kildMessages>> | null> {
    try {
      // The bound and the deadline are one question, asked once: never wait past the window.
      const batch = await kildMessages(
        kildId,
        since,
        watchRequestTimeout(interval, deadline - Date.now()),
      );
      failures = 0;
      answered = true;
      return batch;
    } catch (err) {
      if (++failures < WATCH_TOLERATED_FAILURES) return null;
      console.error(`kild: engine unreachable after ${failures} attempts: ${errText(err)}`);
      process.exit(WATCH_EXIT.unreachable);
    }
  }

  /**
   * Wait for the next poll, or report that the window has closed.
   *
   * The ONE definition of "is there room for another attempt", because there were two and one
   * of them was wrong — twice. First the failure path skipped the wait entirely; then the fix
   * unified failure *tolerance* into `poll()` but left the *deadline* inline at each call
   * site, so the bootstrap fetch had no deadline check at all and a single blip could overrun
   * a one-second window by six seconds. Both bugs were the same shape: one decision, two
   * copies, one of them updated.
   */
  async function waitForNextPoll(): Promise<boolean> {
    if (Date.now() + interval >= deadline) return false;
    await sleep();
    return true;
  }

  function expire(): never {
    // Report the window that actually elapsed. Printing the requested `--timeout` made the
    // message a lie in exactly the case worth reporting — the one where waiting overran it.
    const elapsed = Math.round((Date.now() - started) / 1000);
    if (!answered) {
      // Never heard from it. Saying "nothing new" would assert the engine responded and had
      // nothing, which is the dead-looks-quiet conflation these codes exist to prevent.
      console.error(`kild: engine did not answer within ${elapsed}s`);
      process.exit(WATCH_EXIT.unreachable);
    }
    console.error(`kild: nothing new in ${elapsed}s`);
    process.exit(WATCH_EXIT.quiet);
  }

  // Default to the log's current end, so a watcher started now waits for what happens NEXT
  // rather than firing immediately on the conversation it was started in the middle of. A
  // blip here is tolerated exactly as one in the loop is — starting an instant before an
  // engine finishes restarting must not be reported as an engine that is gone.
  let cursor = parseSince();
  while (cursor === undefined) {
    const batch = await poll(undefined);
    if (batch) cursor = batch.at(-1)?.seq ?? 0;
    else if (!(await waitForNextPoll())) expire();
  }

  // Poll FIRST, then sleep: a watcher handed an explicit `--since` that is already behind the
  // conversation reacts at once instead of after an interval of avoidable silence.
  for (;;) {
    const batch = await poll(cursor);
    if (!batch) {
      // Tolerated failure — wait like any other quiet interval rather than spinning.
      if (!(await waitForNextPoll())) break;
      continue;
    }
    const { incoming, cursor: next } = pollResult(batch, handle, cursor);
    cursor = next;
    if (incoming.length > 0) {
      // Say plainly that nothing was consumed. The obvious wrong assumption about a wake verb
      // is that it also marks-as-seen, and a harness built on that would drop every message it
      // was woken for while looking entirely correct.
      console.log(
        json
          ? JSON.stringify(
              { kildId: id, handle, since: cursor, consumed: false, messages: incoming },
              null,
              2,
            )
          : `${watchSummary(incoming)} (nothing consumed — still in your inbox)\n` +
              // Derived from the OLDEST incoming seq, not from arithmetic on the cursor.
              // `cursor - incoming.length` assumed the new messages were the highest-numbered
              // ones in the batch, so a single own-message with a higher seq made the printed
              // command skip past the very message that woke the watcher — a wrong answer
              // handed over with the confidence of a working command.
              `  read with: kild log ${id} --since ${Math.min(...incoming.map((m) => m.seq)) - 1}\n` +
              `  drain with: kild inbox`,
      );
      process.exit(WATCH_EXIT.mail);
    }
    // Same single definition as the failure path uses: no room for another attempt ends it.
    if (!(await waitForNextPoll())) break;
  }
  expire();
}

async function kildInbox(idArg: string | undefined): Promise<void> {
  const claudeStop = values.format === 'claude-stop';
  // Omitting both resolves the kild this session attached to — the case the environment
  // could never serve, since a session that started before the kild existed has no way to
  // be told about it.
  //
  // This is the one place the hook contract applies: as a hook, an unresolvable or unreadable
  // attachment is silence; as an ordinary verb it is a loud error naming the real cause, not a
  // usage message that sends the operator looking at their own syntax.
  let resolved: { kildId?: string; handle?: string };
  try {
    resolved = await resolveAttachment(idArg, values.as);
  } catch (err) {
    if (!claudeStop) throw err;
    return;
  }
  const { kildId: id, handle } = resolved;
  if (!id || !handle) {
    if (claudeStop) return; // not attached → silence, never a blocked turn
    throw new Error(
      'usage: kild inbox [<id> --as <handle>] [--session <id>] [--format claude-stop]\n' +
        '  omit id/--as to use the kild this session attached to',
    );
  }
  if (values.format !== undefined && !claudeStop) {
    throw new Error(`unknown --format: ${values.format} (supported: claude-stop)`);
  }

  let drained: Awaited<ReturnType<typeof drainInbox>>;
  try {
    drained = await drainInbox(id, handle);
  } catch (err) {
    if (!claudeStop) throw err;
    return; // engine down / unknown kild / timeout — degrade to silence
  }

  if (claudeStop) {
    const output = claudeStopOutput({ kildId: id, handle, messages: drained.messages });
    if (output) console.log(JSON.stringify(output));
    return;
  }
  if (json) return void console.log(JSON.stringify(drained, null, 2));
  for (const message of drained.messages) console.log(`${message.from}: ${message.text}`);
  if (drained.messages.length === 0) console.error(drained.capped ? 'wake cap reached' : 'no mail');
}

/**
 * Run a MUTATING call, and if it times out, say the outcome is unknown rather than failed.
 *
 * A client-side abort does not reach the engine: nothing here passes a signal into the
 * server, and no git command it runs is cancellable, so a timed-out `land` may still merge
 * and a timed-out `rm` may still remove the tree. Reporting that as a plain failure would be
 * the worst kind of wrong — the operator retries a merge that already happened, or treats a
 * deleted worktree as surviving.
 *
 * This is the same rule the disposal path already follows for its discard list: an
 * unanswerable question is not an empty list, and it is not a `no` either.
 */
async function mayHaveHappened<T>(verb: string, call: () => Promise<T>): Promise<T> {
  try {
    return await call();
  } catch (err) {
    // Typed, not string-matched. An engine-side failure whose message merely CONTAINS "timed
    // out" — a relayed git error, a 409 body — must never be reported as "this may have
    // completed", because that sends an operator to check for a merge that never happened.
    if (!(err instanceof EngineTimeout)) throw err;
    throw new Error(
      `${verb} timed out — the engine does NOT cancel work when the client gives up, so this ` +
        `may still be completing or already done. Check with \`kild ls\` and git before ` +
        `retrying. (${errText(err)})`,
    );
  }
}

/** `--since <seq>` as a message cursor. A non-number is a usage error, never "from the
 *  start" — a silently-ignored cursor replays the whole log as if it were new. */
function parseSince(): number | undefined {
  if (values.since === undefined) return undefined;
  const since = Number(values.since);
  if (!Number.isInteger(since) || since < 0) {
    throw new Error(`--since must be a non-negative message seq, got: ${values.since}`);
  }
  return since;
}

/** `kild log <id> [--since <seq>]` — a kild's message thread, from `/messages`. Works for
 *  an archived kild too (its log is the read-only record). `--since` is the cursor: pass the
 *  last `seq` you saw to get only what arrived after it. */
async function kildLog(id: string): Promise<void> {
  const messages = await kildMessages(id, parseSince());
  if (json) return void console.log(JSON.stringify(messages, null, 2));
  for (const m of messages) console.log(`${m.seq}\t${m.from} → [${m.to.join(', ')}]: ${m.text}`);
}

/** `kild show <id>` — one kild's complete coordination and code-state context: the detail
 *  route (identity + git + cost) plus its log, which is a second resource by design. */
async function kildShow(id: string): Promise<void> {
  // The detail route is what decides whether this kild exists; a missing LOG does not mean a
  // missing kild — an orphan tree is a kild with nothing ever said in it.
  const kild = await getKild(id);
  // NOT swallowed. This used to be `.catch(() => [])`, which reported an empty log for any
  // failure — invisible while a hung engine simply hung, and a confident lie the moment a
  // timeout made the call return. An archived kild's log is a real answer; an unreachable
  // engine is not the same as a kild that said nothing.
  const messages = await kildMessages(id);
  const compact = compactLiveKilds([kild])[0];
  if (!compact) throw new Error(`no such kild: ${id}`);

  const detail = {
    id: compact.id,
    name: compact.name,
    agents: compact.agents,
    ...(compact.git ? { git: compact.git } : {}),
    worktree: kild.worktree,
    log: messages,
  };
  if (json) return void console.log(JSON.stringify(detail, null, 2));

  console.log(`${kild.id}\t${kild.name}${kild.orphan ? '\t(orphan tree, no kild record)' : ''}`);
  console.log(`worktree: ${kild.worktree ?? '(none)'}`);
  console.log('agents:');
  for (const agent of compact.agents) {
    const persona = agent.persona ? ` (${agent.persona})` : '';
    const model = agent.model ? ` — ${agent.model}` : '';
    console.log(`  ${agent.handle}${persona}${model}`);
  }
  if (compact.git) {
    console.log(
      `git${formatCompactGitSummary(compact.git)} · changed files: ${compact.git.changedFileCount}${compact.git.error ? ` · error: ${compact.git.error}` : ''}`,
    );
  }
  console.log('log:');
  for (const message of messages) {
    console.log(`${message.seq}\t${message.from} → [${message.to.join(', ')}]: ${message.text}`);
  }
}

/**
 * `kild ls [--state live|orphan|reclaimable] [--git] [--project <p>]` — the kild collection.
 *
 * This lists kilds **and the `kild/*` worktrees git reports**: a tree left by an earlier
 * engine run appears as an `orphan` kild with no agents and no log, addressed by its
 * worktree name. That name is what `kild rm` / `kild land` take, so a stranded tree is
 * reachable again.
 *
 * FAST by default: identity and roster only, from the cheap route — no git, whatever the
 * machine's worktree count. `--git` (and `--state reclaimable`, which is itself a git
 * question) opt into `/api/kilds/status` and pay for branch/ahead/behind/dirty/conflicts and
 * cross-kild collisions.
 */
async function kildList(): Promise<void> {
  const repo = await resolveProjectFlag();
  const withGit = values.git || values.state === 'reclaimable';
  if (!withGit) return kildListCheap(repo);

  const statuses = await kildsStatus({ state: values.state, repo });
  const kilds = compactLiveKilds(statuses);
  const orphaned = new Set(statuses.filter((kild) => kild.orphan).map((kild) => kild.id));
  if (json) {
    return void console.log(
      JSON.stringify(
        kilds.map((kild) => (orphaned.has(kild.id) ? { ...kild, orphan: true } : kild)),
        null,
        2,
      ),
    );
  }
  if (kilds.length === 0) return void console.error('no kilds');
  for (const r of kilds) {
    if (orphaned.has(r.id)) {
      console.log(`${r.id}\t(orphan tree, no kild record)${formatCompactGitSummary(r.git)}`);
      continue;
    }
    const col = r.collidesWith?.length
      ? ` · collides: ${r.collidesWith.map((c) => `${c.kild}(${c.files.length})`).join(', ')}`
      : '';
    console.log(`${r.id}\t${r.name} [${roster(r.agents)}]${formatCompactGitSummary(r.git)}${col}`);
  }
}

/** The default `kild ls`: identity only, and therefore no git subprocesses at all. */
async function kildListCheap(repo: string | undefined): Promise<void> {
  const kilds = await listKilds({ state: values.state, repo });
  if (json) return void console.log(JSON.stringify(kilds, null, 2));
  if (kilds.length === 0) return void console.error('no kilds');
  for (const kild of kilds) {
    if (kild.orphan) {
      console.log(`${kild.id}\t(orphan tree, no kild record)`);
      continue;
    }
    const tree = kild.worktree ? ` · ${kild.worktree}` : '';
    console.log(`${kild.id}\t${kild.name} [${roster(kild.agents)}]${tree}`);
  }
}

/** One kild's roster, as `kild ls` prints it. */
function roster(agents: Array<{ handle: string; model?: string; stopped?: boolean }>): string {
  return agents
    .map((a) => `${a.model ? `${a.handle}:${a.model}` : a.handle}${a.stopped ? ' (stopped)' : ''}`)
    .join(', ');
}

/** `kild spawn <id> --as <handle> [--persona p] [--model m] [--task <text>]` — add an agent
 *  to a live kild. `--task` is its first message (attributed to the human, like every other
 *  send from this CLI); without one the agent starts idle until something sends to it.
 *  Answers: a rejection (unknown persona, duplicate handle, capacity) is an error with the
 *  engine's reason, not a shrug. */
async function kildSpawn(id: string, handle: string): Promise<void> {
  const res = await spawnKildAgent(id, {
    handle,
    persona: values.persona,
    model: values.model,
    task: values.task,
  });
  console.log(json ? JSON.stringify(res, null, 2) : res.message);
}

/**
 * `kild rm <id|worktree-name> [--force]` — dispose of a kild's worktree.
 *
 * Refused when the branch carries commits the base does not have: that is unlanded work,
 * and you are told so instead of the tree quietly surviving forever. Uncommitted files are
 * NOT a refusal (provisioning litter is not work) — they are discarded and listed. The
 * `kild/<name>` branch always survives, which is why `--force` costs no commits.
 */
async function kildRm(id: string): Promise<void> {
  const res = await mayHaveHappened('rm', () => disposeKild(id, values.force));
  if (json) return void console.log(JSON.stringify(res, null, 2));
  console.log(res.message);
  // An unanswerable question is not an empty list, and this is the one moment the operator
  // could still have acted on the difference.
  if (res.discardedError) console.error(`discarded: UNKNOWN (git: ${res.discardedError})`);
  else if (res.discarded.length > 0) console.error(`discarded: ${res.discarded.join(', ')}`);
}

function formatLand(res: LandResponse): string {
  const head = `${res.branch ?? '(no branch)'} → ${res.base}`;
  const commits = `${res.commits.length} commit${res.commits.length === 1 ? '' : 's'}`;
  const files = `${res.files.length} file${res.files.length === 1 ? '' : 's'}`;
  if (res.merged) return `landed ${head} as ${res.sha?.slice(0, 7)} (${commits}, ${files})`;
  if (res.error) return `would not land ${head}: ${res.error}`;
  return `would land ${head}: ${commits}, ${files}`;
}

/** `kild land <id|worktree-name> [--execute]` — without `--execute` this is a DRY RUN
 *  that touches nothing; with it, the branch is merged into its base in the project's main
 *  checkout and the merge sha is reported (and recorded on the kild for the ledger). */
async function kildLand(id: string): Promise<void> {
  const res = values.execute
    ? await mayHaveHappened('land', () => landKild(id))
    : await landPreview(id);
  if (json) return void console.log(JSON.stringify(res, null, 2));
  console.log(formatLand(res));
  if (res.collides.length > 0) console.error(`collides: ${res.collides.join(', ')}`);
  if (!res.merged && !values.execute && res.wouldMerge) {
    console.error('(dry run — re-run with --execute to merge)');
  }
}

/** `kild stop <id>` with `--as <handle>` stops ONE agent instead of the whole kild. */
async function kildStopAgent(id: string, handle: string): Promise<void> {
  const res = await stopKildAgent(id, handle);
  console.log(json ? JSON.stringify(res, null, 2) : res.message);
}

/** `kild new <goal> --detach` — open a kild, print its id, return (no streaming). */
async function kildNew(goal: string): Promise<void> {
  if (!goal) throw new Error('usage: kild new <goal…> [--agents a,b] [--to a] [--detach]');
  if (!values.detach) return kildInteractive(goal);
  const agents = kildAgents();
  // The kickoff names its recipients like any other message. With one agent the CLI
  // resolves that here; with several, say who the goal is for.
  const to =
    parseTo() ??
    soleRecipient(
      agents.map((a) => a.handle),
      'this kild',
    );
  const res = await newKild({
    name: values.project ?? 'kild',
    cwd: await kildCwd(),
    agents,
    worktree: values.worktree,
    base: values.base,
    kickoff: { to, text: goal },
  });
  console.log(json ? JSON.stringify(res, null, 2) : res.id);
}

/** `--to a,b` as a handle list. Returns undefined when the flag was not given at all;
 *  a given-but-empty `--to` is a usage error, never "everyone". */
function parseTo(): string[] | undefined {
  if (values.to === undefined) return undefined;
  const to = values.to
    .split(',')
    .map((s) => s.trim().replace(/^@/, ''))
    .filter(Boolean);
  if (to.length === 0) throw new Error('--to must name at least one agent, e.g. --to claude');
  return to;
}

/**
 * The CLI's one convenience the ENGINE deliberately refuses: in a kild with exactly one
 * agent there is no ambiguity about who a message is for, so `--to` may be omitted and
 * resolved here — client-side, against the roster this CLI can see — then passed
 * explicitly. The engine still never defaults; it is handed a named recipient either way.
 */
function soleRecipient(handles: string[], where: string): string[] {
  const only = handles[0];
  if (handles.length === 1 && only) return [only];
  throw new Error(
    handles.length === 0
      ? `--to is required: ${where} has no agents to address`
      : `--to is required: ${where} has ${handles.length} agents (${handles.join(', ')})`,
  );
}

/** `kild send <id> --to a,b <text>` — steer an existing kild from a separate call.
 *  `--to` names the agents addressed, mirroring the in-kild `send` tool. Recipients are
 *  never parsed from the text, so writing `@handle` in the message body addresses
 *  nobody. */
async function kildSend(id: string, text: string): Promise<void> {
  let to = parseTo();
  if (!to) {
    // The cheap listing carries the roster, which is all a sole-recipient resolution needs.
    const kild = (await listKilds({ state: 'live' })).find((candidate) => candidate.id === id);
    if (!kild) throw new Error(`no such live kild: ${id}`);
    to = soleRecipient(
      kild.agents.map((agent) => agent.handle),
      `kild ${id}`,
    );
  }
  const res = await sendMessage(id, to, text, undefined, await attachedSendToken(id));
  if (json) console.log(JSON.stringify(res, null, 2));
  else console.error(res.message);
}

/**
 * The attached-harness credential for a send, when this invocation is one.
 *
 * Without it an attached sender presents nothing and lands on the log as the unattributed
 * label — indistinguishable from the operator typing, and from every other attached agent in
 * the kild. That is the whole bug: the engine has minted this credential at `attach` all
 * along and the CLI threw it away.
 *
 * Three cases produce no token, all of them correct:
 *  - an agent the engine SPAWNED — it proves itself with `KILD_AGENT_ID`, and the engine sets
 *    `KILD_KILD_ID`/`KILD_HANDLE` on it too, so without this guard a spawned agent would
 *    attach a second, shadow copy of itself over its own handle;
 *  - a send to any kild other than the one this session attached to — a handle is unique
 *    within one kild and means nothing outside it, so there is no identity to present;
 *  - a plain human's shell, which has no attachment at all.
 *
 * Minting is not guarded: if we resolved a handle for this very kild and `attach` then fails,
 * that is a real failure to report. Falling back to sending unattributed would silently
 * reintroduce the bug at exactly the moment it matters.
 */
async function attachedSendToken(kildId: string): Promise<string | undefined> {
  if (process.env.KILD_AGENT_ID) return undefined;
  const { kildId: attachedTo, handle } = await resolveAttachment(undefined, values.as);
  if (!handle || attachedTo !== kildId) return undefined;
  return (await attachAgent(kildId, handle)).token;
}

/** `kild stop <id>` — stop a specific kild by id. With `--as <handle>` it stops that ONE
 *  agent and leaves the kild running. */
async function kildStop(id: string): Promise<void> {
  if (values.as) return kildStopAgent(id, values.as);
  const res = await stopKildRequest(id);
  if (json) console.log(JSON.stringify(res, null, 2));
  else console.error(res.message);
}

async function kildInteractive(goal: string): Promise<void> {
  if (!goal) {
    throw new Error('usage: kild new <goal…> [--agents a,b,c] [--worktree <n>] [--project <p>]');
  }
  const engineUp = await fetch(`${ENGINE}/api/health`)
    .then((r) => r.ok)
    .catch(() => false);
  if (!engineUp) {
    throw new Error(`engine not running at ${ENGINE} — start it: cd engine && bun run dev`);
  }
  const cwd = (await resolveProjectFlag()) ?? process.cwd();
  const name = values.project ?? 'kild';
  // `--agents a,b` names personas from the project's own persona files; each
  // agent's persona defaults to its handle. With none given, kild opens one
  // general-purpose agent (the `default` persona = kild's system prompt, no persona) —
  // kild ships no personas of its own; personas come from the project
  // (.claude/agents / .pi/agents — the dir names are upstream convention).
  const agents = values.agents
    ? values.agents
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
        .map((n) => ({ handle: n, persona: n }))
    : [{ handle: 'agent', persona: GENERAL_PERSONA }];
  if (agents.length === 0) throw new Error('--agents must name at least one agent');
  const base = values.base;
  // Every message this session sends — the kickoff and each line typed afterwards — goes
  // to these handles. The engine never infers them; the CLI resolves a single-agent kild
  // as a convenience and otherwise makes you say who you are talking to.
  const to =
    parseTo() ??
    soleRecipient(
      agents.map((a) => a.handle),
      'this kild',
    );
  const ws = new WebSocket(`${ENGINE.replace(/^http/, 'ws')}/ws`);

  // Subscribe BEFORE creating, so nothing said in the first instants is missed; then create
  // over REST, which ANSWERS.
  //
  // Creation used to be a `kild_new` frame: fire-and-forget, rejected with a `console.warn`
  // inside the engine, invisible to the caller. `kild new --agents not-a-persona` printed a
  // confident header inviting you to type into a kild that never existed, and then hung
  // forever. That is the exact silent-failure shape the REST spawn route exists to remove —
  // it was fixed on the route and left on the transport the default verb actually used.
  //
  // So: the socket is a SUBSCRIPTION, and every mutation goes over REST where a rejection is
  // a rejection. Nothing needs a rejection frame, because nothing mutates over the socket.
  await new Promise<void>((resolve, reject) => {
    ws.addEventListener('open', () => resolve());
    ws.addEventListener('error', () => reject(new Error(`engine socket error at ${ENGINE}`)));
  });

  let kildId: string;
  try {
    kildId = (
      await newKild({
        name,
        cwd,
        worktree: values.worktree,
        base,
        agents: agents.map((p) => ({ ...p, model: values.model })),
        kickoff: { to, text: goal },
      })
    ).id;
  } catch (err) {
    ws.close(); // nothing to stream: the kild does not exist
    throw err;
  }

  await new Promise<void>((resolve, reject) => {
    const stopKild = () => {
      void stopKildRequest(kildId)
        .catch((err) => console.error(`\x1b[31mstop failed:\x1b[0m ${errText(err)}`))
        .finally(() => {
          ws.close();
          resolve();
        });
    };
    process.on('SIGINT', () => {
      if (!json) console.error('\n\x1b[2m— stopping kild —\x1b[0m');
      stopKild();
    });

    // Mid-flight steering: every line you type is sent into the kild, unconditionally.
    //
    // There is DELIBERATELY no slash-command here. A `/spawn <handle>` shortcut used to be
    // matched out of the input first, which meant a line you typed could be swallowed and
    // never delivered — and whether it was depended on its word count, since the pattern
    // only accepted up to three tokens. Nothing should read your prose to decide what to
    // do with it; that is the bug this whole restructure exists to remove, and it applies
    // to a human's words as much as an agent's.
    //
    // What you type is a message. Always. To add an agent, run `kild spawn <id> --as
    // <handle>` — the id is printed above, and a real command reports a real error.
    if (!json) {
      process.stdin.setEncoding('utf8');
      process.stdin.on('data', (chunk: string) => {
        for (const raw of chunk.split('\n')) {
          const text = raw.trim();
          if (!text) continue;
          // Over REST for the same reason as create: a line you typed that nobody could
          // receive must say so, not vanish into a server-side log line.
          void sendMessage(kildId, to, text).catch((err) =>
            console.error(`\x1b[31mnot sent:\x1b[0m ${errText(err)}`),
          );
        }
      });
    }

    if (!json) {
      const where = values.worktree ? ` · tree kild/${values.worktree}` : '';
      console.error(
        `\x1b[2m# kild "${name}" — ${agents.map((p) => p.handle).join(', ')}${where} · ${kildId}\n` +
          `# type to send to ${to.map((h) => `@${h}`).join(' ')} · Ctrl-C to stop\x1b[0m`,
      );
    }

    ws.addEventListener('message', (e) => {
      let parsed: {
        message?: { kildId: string; from: string; to: string[]; text: string };
        archivedKild?: { id: string };
      };
      try {
        parsed = JSON.parse(String((e as { data: unknown }).data));
      } catch {
        return;
      }
      // The engine archived our kild (an agent called `stop`, or another
      // client stopped it) — the run is over; resolve without re-stopping.
      if (parsed.archivedKild?.id === kildId) {
        if (!json) console.error('\x1b[2m— kild stopped —\x1b[0m');
        ws.close();
        resolve();
        return;
      }
      const m = parsed.message;
      if (!m || m.kildId !== kildId) return;
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
    // Settle exactly once, then tear the agent down. Without this the CLI hung
    // forever: it only resolved on `agent_end` and only rejected on a socket
    // `error`, so an agent-emitted `error`/`session_end` or a *graceful* close
    // (the engine restarting under `--watch`) left the run waiting on nothing.
    let settled = false;
    const finish = (err?: Error) => {
      if (settled) return;
      settled = true;
      try {
        ws.send(JSON.stringify({ type: 'kild_stop', id })); // one-shot: tear the kild down
      } catch {
        // socket already closing/closed — nothing to close
      }
      ws.close();
      if (err) reject(err);
      else resolve();
    };

    ws.addEventListener('open', () => {
      // A one-shot run is a 1-agent kild, so the CLI resolves the recipient here and
      // addresses it explicitly — @agent is the whole roster.
      ws.send(
        JSON.stringify({
          type: 'kild_new',
          id,
          name: values.project ?? 'run',
          cwd: projectPath ?? process.cwd(),
          worktree: values.worktree,
          agents: [{ handle: 'agent', persona: values.persona, model: values.model }],
        }),
      );
      ws.send(JSON.stringify({ type: 'kild_send', id, to: ['agent'], text: prompt }));
    });
    ws.addEventListener('message', (e) => {
      const msg = JSON.parse(String((e as { data: unknown }).data)) as {
        kild?: string;
        agent?: string;
        event?: { kind: string; [k: string]: unknown };
      };
      if (msg.kild !== id || !msg.event) return;
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
        finish(); // agent ended (normally after our stop, or abnormally) — don't hang
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
 * Run through the same JSONL agent boundary as the engine. `server.ts` owns the
 * agent role dispatch, so invoke that entry explicitly rather than re-invoking this
 * CLI entry (which would otherwise start a second CLI process under `KILD_ROLE`).
 */
async function runViaAgent(prompt: string): Promise<void> {
  const projectPath = values.project ? (await findProject(values.project))?.path : undefined;
  const agentEntry = fileURLToPath(new URL('./server.ts', import.meta.url));
  const child = spawn(process.execPath, [agentEntry], {
    env: {
      ...process.env,
      KILD_ROLE: 'agent',
      KILD_CWD: projectPath ?? process.cwd(),
      KILD_PERSONA: values.persona ?? '',
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
        finish(new Error(`agent emitted invalid JSONL: ${line}`));
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
          finish(new Error(String(event.message ?? 'agent error')));
          break;
        case 'agent_end':
          agentEnded = true;
          // The agent writes stats immediately after agent_end. Defer stopping until
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
    child.on('error', (err) => finish(new Error(`agent failed: ${err.message}`)));
    child.on('close', (code) => {
      if (buffer.trim()) consume(buffer.trim());
      if (!agentEnded)
        finish(new Error(`agent exited before completing (code ${code ?? 'unknown'})`));
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
