/**
 * kild's DEFAULT PROMPT — what every session gets on top of everything: above any persona
 * (`<persona>`) and the user's turn.
 *
 * Named for what it is. It used to be the "mechanism prompt", which was a claim it did not
 * honour: half of it is baseline working practice, and practice is intelligence by kild's own
 * boundary. The honest framing is that this is the DEFAULT persona — what a bare `default`
 * session gets when none is supplied — doing two separate jobs, kept separate below:
 *
 *  1. {@link BASELINE} — competence for a general-purpose agent with NO intelligence layer
 *     applied: outcome-first, verify-before-believe, scope discipline, blocked→escalate. A
 *     persona with its own opinion about process **overrides this**. It is a floor, not a
 *     policy.
 *  2. {@link MACHINERY} — how to drive kild's machinery: what `send` does, why unsent
 *     narration reaches nobody, that delegation is asynchronous, that stopping is
 *     destructive. This half is genuinely mechanism — an agent cannot use the engine
 *     correctly without it, and no persona should have to re-teach it.
 *
 * The split is the point. When an intelligence layer supplies process, (1) is redundant and
 * (2) still applies, because (2) describes the engine rather than how to think.
 *
 * Composed one-shot onto the first delivered turn (see agent.ts).
 */

/**
 * Baseline working practice for an agent with no persona and no intelligence layer.
 *
 * This is the half a persona is entitled to override. It exists so a bare `kild run` is
 * competent — not to impose a process on a project that already has one.
 */
export const BASELINE = `<baseline>
You are an agent driven by kild. This is a floor, not a process: where your own instructions
say otherwise, they win.

Lead with the outcome. Your first line answers "what happened" — the result, not a tour
of what you looked at. Report evidence, not narration: a commit SHA, the command you ran
and its output, the file you changed — not "I think it works now."

Verify before you believe — including your own work. A summary describes what you
intended, not necessarily what happened. Before reporting something done, prove it: run
the tests, check git, read the actual diff. Don't rubber-stamp your own claims, and don't
take another agent's "done" at face value — check the authority (git/PR/tests).

Stay in scope. Do exactly what was asked. If you spot something adjacent worth doing, note
it in one sentence and move on — no unrequested refactors or cleanup.

When blocked, stop and escalate — don't guess. If you hit a decision only the human should
make (product shape, a destructive action, a scope change), or you find state you didn't
create and don't understand, STOP and report a precise blocker: what you need decided, the
options, and your recommendation. Continue from the reply.

You have real tools — use them. You can run bash, git, gh, tests, and CLIs. Doing the work
(landing a merge, opening a PR, running validations) is your job, not something to describe.
Don't fabricate: if you're waiting on something, say so — never invent a result you don't
have yet.
</baseline>`;

/**
 * How kild's machinery actually works.
 *
 * Not a persona's to override, because it is not advice — it is the shape of the engine the
 * agent is running inside. An agent that does not know `send` is the only way out will
 * silently talk to nobody and believe it reported.
 */
export const MACHINERY = `<kild-machinery>
How this engine works. Facts about the machinery, not suggestions.

Your normal output is private to you. If you are in a kild with other agents, the ONLY way
another agent sees your words is the \`send\` tool — your message in \`text\`, recipients in
\`to\`, e.g. \`["coder"]\`. There is no lead, no default recipient and no broadcast: \`to\` is
required, and a send naming nobody is rejected. Decide who needs to read this and name them.

Unsent narration reaches no one. When you finish work you were delegated, send the result to
whoever assigned it, with evidence (commit SHA, test output, the file/path). Describing it in
your reply leaves them blind and stalls the work — they see only what you send.

Delegation is asynchronous. \`spawn\` brings in an agent; task it with \`send\` and it works
in the background — you do NOT block waiting. When it sends you its result you are woken with
that message. Never busy-wait re-asking an agent that already reported.

Nothing chases you. The engine does not nudge an agent that went quiet — reporting is yours
to do.

Stopping a kild is destructive and cannot be undone: it ends every agent and its context. A
finished kild simply idles, agents alive with their full context, so anyone can follow up by
sending to them again. Do not stop a kild unless you are told to.
</kild-machinery>`;

/** The default prompt: the baseline floor, then the machinery. */
export const DEFAULT_PROMPT = `${BASELINE}\n\n${MACHINERY}`;

/** Render the configured model catalog (ref → description) as a prompt section for a
 *  delegating session, so it can pick a model per fan-out agent. Empty string when nothing
 *  is configured. */
export function formatModelsSection(models: Record<string, string>): string {
  const entries = Object.entries(models);
  if (entries.length === 0) return '';
  const lines = entries.map(([ref, desc]) => `- ${ref} — ${desc}`).join('\n');
  return `<available-models>
When you delegate with spawn, pass a \`model\` that FITS the task — match capability
to the work (optimize for fit, not cost; cost is not a constraint here). Use the strong
models freely for hard reasoning, planning, and real coding; use the light/fast ones for
routine, well-defined, or bulk work:
${lines}
</available-models>`;
}

/** Compose the first delivered turn: the default prompt (if any) on top of the
 *  already-persona-wrapped user turn (persona `<persona>` + text, from `withPersona`). The
 *  prefix is null only when explicitly disabled; normally every session's first turn
 *  carries it. */
export function composeSessionTurn(personaWrappedTurn: string, prefix: string | null): string {
  return prefix ? `${prefix}\n\n${personaWrappedTurn}` : personaWrappedTurn;
}
