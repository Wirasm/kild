import { spawn as spawnProcess } from 'node:child_process';
import { GENERAL_PERSONA } from './personas.ts';

/**
 * Lifecycle hooks — the mechanism half of "something should happen when a kild ends".
 *
 * The engine holds facts (which kild, where it ran, where its transcript and ledger
 * landed) and it holds the moment a kild ENDS — stop only; landing does not end a kild, and
 * no hook fires on it. What should HAPPEN at that moment is
 * never the engine's call: a hook is declared in config (`hooks.onClose`) and the engine
 * runs it verbatim. kild therefore has no idea what any hook is *for* — no hook name is
 * special-cased, no hook payload is interpreted, and nothing here knows what an
 * intelligence layer might do with the facts.
 *
 * Two declarable forms, both minimal:
 * - `agent` — spawn one agent (persona + model) and hand it a prompt. This is the shape
 *   an intelligence layer needs: its charter is the prompt, its judgment is the persona.
 * - `command` — run an argv (no shell), with the same facts in the environment. This is
 *   the shape a script needs.
 *
 * Declaring both runs both. Declaring neither is a no-op.
 */

/** The facts the engine holds about a kild that just closed. Everything here is
 *  ground truth the engine wrote or resolved itself — never a summary, never a guess. */
export interface KildCloseEvent {
  kildId: string;
  name: string;
  /** The project checkout the kild ran against (never a worktree). */
  cwd: string;
  /** Shared worktree name, when the kild had one. */
  worktree?: string;
  /** Base branch the kild measured against. */
  base?: string;
  /** Persisted kild transcript (JSON) — the full message history. */
  transcriptPath: string;
  /** The engine-written ledger the close entry was just appended to. */
  ledgerPath: string;
}

/** Spawn-an-agent hook form: a persona/model and the text it is handed. */
export interface HookAgent {
  persona?: string;
  model?: string;
  /** The task text. `{{kildId}}` `{{name}}` `{{cwd}}` `{{worktree}}` `{{base}}`
   *  `{{transcriptPath}}` `{{ledgerPath}}` are substituted from the event. */
  prompt: string;
}

/** One declared hook. Opaque to the engine beyond these two run shapes. */
export interface KildHook {
  agent?: HookAgent;
  /** argv (no shell). Each element gets the same `{{fact}}` substitution as a prompt. */
  command?: string[];
}

/** What the engine needs to start a hook agent — deliberately the same fields a normal
 *  agent spawn takes, so a hook is not a privileged kind of session. */
export interface HookAgentSpawn {
  model?: string;
  cwd: string;
  persona: string;
  label: string;
}

/** How the caller starts a hook agent (the engine's session substrate, injected so this
 *  slice owns no process management of its own). */
export type SpawnHookAgent = (spec: HookAgentSpawn, prompt: string) => void;

/** Facts as flat strings — one table feeding both `{{fact}}` substitution and the env. */
function facts(event: KildCloseEvent): Array<[string, string]> {
  return Object.entries(event)
    .filter((entry): entry is [string, string] => typeof entry[1] === 'string')
    .map(([key, value]) => [key, value]);
}

/** Substitute `{{fact}}` placeholders from the close event. An unknown placeholder is
 *  left verbatim — the engine does not own the hook's text and must not eat parts of it.
 *  Deliberately NOT `${...}`: hook text is often a shell argv, and shell expansion is the
 *  hook author's, not ours. */
export function renderHookText(text: string, event: KildCloseEvent): string {
  let out = text;
  for (const [key, value] of facts(event)) out = out.split(`{{${key}}}`).join(value);
  return out;
}

/** Close-event facts as environment for a command hook: `kildId` → `KILD_CLOSE_KILD_ID`,
 *  `transcriptPath` → `KILD_CLOSE_TRANSCRIPT_PATH`. Absent facts are simply absent. */
export function hookEnv(event: KildCloseEvent): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [key, value] of facts(event)) {
    env[`KILD_CLOSE_${key.replace(/[A-Z]/g, (c) => `_${c}`).toUpperCase()}`] = value;
  }
  return env;
}

/** Run a command hook. No shell: argv only, exactly like every other git/process call in
 *  the engine, because hook text can come from a config an agent wrote. Resolves when the
 *  command exits; a non-zero exit or a spawn failure is logged, never thrown — a hook must
 *  not be able to break the lifecycle event that fired it. */
function runCommand(argv: string[], event: KildCloseEvent): Promise<void> {
  const [bin, ...args] = argv.map((part) => renderHookText(part, event));
  return new Promise((resolve) => {
    const child = spawnProcess(bin as string, args, {
      cwd: event.cwd,
      env: { ...process.env, ...hookEnv(event) },
      stdio: 'ignore',
    });
    // A failed spawn fires BOTH `error` and `close`; report the first outcome only.
    let reported = false;
    child.on('error', (err) => {
      if (!reported) console.error(`kild: onClose hook failed for '${event.name}': ${err.message}`);
      reported = true;
      resolve();
    });
    child.on('close', (code) => {
      if (!reported && code !== 0) {
        console.error(`kild: onClose hook for '${event.name}' exited ${code}`);
      }
      reported = true;
      resolve();
    });
  });
}

/**
 * Run whatever the hook declares against one close event.
 *
 * Resolves once every declared form has finished starting (agent) or exited (command),
 * so a caller can await it in a test; the lifecycle path fires it without awaiting so a
 * slow hook can never hold up a stop.
 */
export async function runCloseHook(
  hook: KildHook,
  event: KildCloseEvent,
  spawnAgent: SpawnHookAgent,
): Promise<void> {
  if (hook.agent?.prompt) {
    spawnAgent(
      {
        model: hook.agent.model,
        // The MAIN checkout: a hook runs after the kild's agents are gone, and the
        // worktree may be pruned out from under it.
        cwd: event.cwd,
        persona: hook.agent.persona ?? GENERAL_PERSONA,
        label: `onClose:${event.name}`,
      },
      renderHookText(hook.agent.prompt, event),
    );
  }
  if (hook.command && hook.command.length > 0) await runCommand(hook.command, event);
}
