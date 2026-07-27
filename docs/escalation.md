# Escalation — when an agent needs a human

`cross-kild-comms.md` ends by naming this gap without answering it. This answers it.

Removing the human from the transport does not remove them from the loop. In the exchange that
motivated cross-kild comms, the human was not only relaying — they were deciding. Once agents
talk directly, *"this needs a human call"* has to be something an agent can say and something
that reliably reaches one.

## The split

| Layer | Owns |
|---|---|
| **PRP** | *Raising.* What counts as a blocker, when to stop rather than guess, how to state the options and a recommendation. |
| **kild** | *Carrying.* The message, the recipient, and the observable fact that an agent is idle after sending. |
| **helm** | *Surfacing.* Making an unanswered escalation impossible to miss, and answerable in one place. |

**helm owns the surfacing permanently, not as a holding position.** A decision is the one thing
kild does that is worthless if nobody sees it — everything else the engine does is designed to
run unattended. Reaching a screen, making noise, and persisting until acknowledged is a client's
job by definition, and helm is the client already watching.

## kild adds nothing

This is the point worth defending. kild already has everything required:

- **`idle`** — an agent that went idle after sending to the honryo *is* a blocked agent. This is
  exactly why `idle` survived the process-norms cut: it is observable state, not a norm.
- **the message log** — what was asked, by whom, in which kild.
- **`seq`** (planned, `api-surface.md` §5) — a monotonic cursor so a client can distinguish "new
  since I last looked" from "the same thing I already saw." `ts` is `Date.now()` and can go
  backwards, so it cannot do this.

There is **no escalation type, no engine verb, no priority flag.** Deciding that a message is a
question rather than a status update is interpretation, and interpretation is intelligence. The
keyed-decision ledger was removed from the engine for parsing exactly that out of prose;
re-adding it as a first-class "escalation" is the same mistake with a better name.

## The requirement helm must meet

An escalation nobody answers is worse than no escalation: the agent is blocked, the kild is
burning nothing, and it looks identical to *still working*.

That is the shape of the worktree disposal leak — a silent failure that only became visible when
someone counted, at 116 trees. The lesson transfers directly:

> **Unanswered must be loud, and it must be countable.**

If helm cannot answer *"how many agents are blocked on me right now?"* at a glance, the mechanism
is not finished. A list is not enough; a count that is zero when idle and non-zero when someone
is waiting is the thing that makes the failure mode visible before it accumulates.

## Cheapest first version

Render blocked-on-you agents as **a count plus a list**, from `idle` and the message log helm
already reads. No engine change required beyond `seq`, which is already planned for other
reasons.

If that turns out to be sufficient, it cost nothing. If it is not, the gap will be specific and
observed rather than guessed.

## On a second app

Deferred deliberately, not rejected.

A dedicated app becomes the right answer if escalations need to reach a human when helm is *not*
open — a phone, a notification, away from the machine. That is a plausible and genuinely
different product.

But it is a conclusion to reach *after* helm's version reveals how often this fires and how
urgent it feels in practice. Building it first means guessing at both. The cheap version is also
the instrument that tells you whether the expensive one is needed.

## Open question

**Does an escalation need an explicit answer, or does any reply resolve it?** An agent that asks
a question and receives a message has been answered, mechanically — but a human who glances and
sends *"looking into it"* has not decided anything, and the count should arguably stay lit.

That distinction is interpretation again, which puts it in PRP rather than the engine. Worth
deciding before helm builds the count, since it determines what the count counts.
