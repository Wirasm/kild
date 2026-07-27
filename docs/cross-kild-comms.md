# Cross-kild comms — plan

Agents in different kilds cannot reach each other. A human copies messages between them. This
plans the removal of that human from the transport.

Design only. Nothing here is built.

## The problem, stated precisely

Two agents — often in two repos — are working on coupled changes. One reviews, one implements.
Every exchange goes: agent A produces output → human copies it → human pastes to agent B →
agent B responds → human copies back. The human is a message bus with a keyboard.

This is not only a cross-repo problem. Two kilds on **one** repo have it too: the agent in
`kild/auth` that discovers an API change affecting `kild/api` has no way to say so. It reports
to a human who relays. Parallel workstreams that cannot talk are only half-parallel.

## The principle: widen the address, do not add a channel

The tempting design is a kild-to-kild channel — a second messaging system for traffic that
crosses a boundary. **Reject it.** The endpoint is not a kild, it is an agent. What is missing
is not a channel but an *address space*: `send` can name a recipient in its own kild and nowhere
else.

So: one mechanism, widened. `send` learns a two-part address. Delivery, inboxes, and the loop
guard are untouched.

This matters because a second system would need its own delivery rules, its own guard, and its
own log — and would immediately drift from the first. The whole reframe exists because a second
answer to "who is this addressed to?" was allowed to exist.

## What already exists

More than expected. From `inbox.ts`, on the wake cap:

> *"THE loop guard: waking an attached harness costs the owner real credits, and **two attached
> participants replying to each other would otherwise wake each other forever.** It lives here —
> in the engine — so every harness inherits it and no hook can opt out."*

The primary failure mode of agent-to-agent comms already has a guard, deliberately placed where
nothing can opt out of it. Also present: `attach` (a handle for a harness kild does not own),
directed `send`, destructive `drain`, and per-agent inboxes.

## Address grammar

**Be explicit. Do not make it polymorphic.** This codebase has already paid for the alternative
twice: `--project` accepted a name-or-path union and silently resolved an unregistered name
against the cwd, opening a kild whose worktree could not be created while still returning
success. `server.ts` fixed it by demanding `project` *or* `path`, never guessing — *"the old
polymorphic name-or-path guessing is gone."*

So the wire format is structured and unambiguous:

```
to: [ { kild: "<kild-id>", handle: "<handle>" } ]
```

- Omitting `kild` means the sender's own kild — the common case stays terse.
- `kild` is an **id**, never a name. Kild names are not unique across an engine and never have
  been; resolving a name in the engine would reintroduce exactly the guessing that was removed.

**Ergonomics live at the edge.** The `send` tool and the CLI accept a friendly `name/handle`
form and resolve it to an id *before* calling, using the same listing an agent can already read.
One resolution point, at the caller, where a failure is the caller's to see.

## Message shape

A cross-kild message has a sender kild and one or more recipient kilds. Both belong on the
record:

```
Message {
  id, seq, ts, text
  from: { kild, handle }
  to:   [ { kild, handle } ]
}
```

**Log projection:** a kild's log is every message where that kild is the sender's *or* any
recipient's. A cross-kild message therefore appears in both logs without being stored twice —
an observer of either side sees the exchange, and neither sees a dangling half.

## Delivery and the loop guard

Delivery is unchanged: owned agents are pushed to (prompt on stdin), attached agents are queued
for and pull at their turn boundary. Crossing a kild boundary changes the address, not the
transport.

**One change is required, and it is one the router rewrite already implies:** generalize the
wake cap from attached-only to every agent. Today it guards attached inboxes because only
attached agents could ping-pong. Once every agent has an inbox and any agent can address any
other, two *owned* agents in different kilds can loop just as easily — and they cost more,
because nobody is watching a kild the way a human watches their own terminal.

The primitive already exists and is already engine-side. It needs to stop being conditional.

## Authorization

On a single-user loopback engine, every agent belongs to the same human, so cross-kild send is
allowed by default. Adding a permission model here would be intelligence creeping into
mechanism.

What is worth having is **accounting**: cross-kild traffic should be visible to the seat, since
a runaway agent broadcasting into every kild is a real failure mode and the wake cap bounds the
loop, not the fan-out. This composes with the credential minted at `attach` (see
`api-surface.md` §4) — a cross-kild send is attributable to a handle, or it is not delivered.

## Out of scope: cross-engine

Two engines on two machines — a remote container and a laptop — is a different problem:
different trust domain, a network, real authentication, delivery guarantees under partition.
None of that is needed for the case that matters, where one engine already serves every project
on the machine.

Explicitly not designed here. If it is ever wanted, it is a federation layer *above* this one,
not a change to it.

## Available today, as a stopgap

No new mechanism is required to stop the ping-pong right now, because **an attached agent does
not run in its kild's worktree** — its process belongs to the human. So a kild can serve as a
pure channel:

1. Each session runs `attach` against one shared kild, claiming a handle
2. They `send` to each other by handle
3. Each drains at its turn boundary (the Stop hook already does this)

The kild's worktree simply goes unused. This works, and the wake cap bounds it.

It is a stopgap and should be named as one: it borrows a workstream to use as a mailing list,
and it does not help *owned* agents in separate kilds, which is the case that motivates the real
design.

## Open questions

1. **Does a cross-kild send need the recipient kild to be live?** Sending into a stopped kild
   should fail loudly rather than queue into a void — but an archived kild's agents are gone
   entirely, which is a different error. Two failure modes, two messages.
2. **Fan-out limit.** The wake cap bounds a loop between two agents. Nothing bounds one agent
   addressing thirty kilds at once. Probably a per-turn recipient cap, but the number wants
   evidence rather than a guess.
3. **Does `handle` need to be unique per engine, or only per kild?** Per-kild is enough for
   addressing (the kild id disambiguates) and is what the roster already guarantees. Per-engine
   uniqueness would allow bare `@handle` cross-kild addressing at the cost of a global namespace
   — probably not worth it, but it is the one call that would simplify the grammar.
4. **Where does the review protocol live?** The loop this exists to automate — propose, review,
   cite evidence, correct — is a *process*, and processes are PRP's. kild carries messages; PRP
   decides what a review is and when to escalate to the human. Worth stating plainly so the
   engine never grows a "review" verb.

## The human is still needed — for a different thing

Removing the human from the transport does not remove them from the loop. In the exchange this
plans to automate, the human was not only relaying: they were deciding — settling
handle-vs-id, accepting a correction, choosing what to defer.

Those calls need an explicit escalation path, and it does not belong in the engine. The keyed
decision protocol was just removed from kild for exactly that reason and moved to PRP. Direct
agent-to-agent comms makes that protocol **more** load-bearing, not less: the moment agents stop
routing through a human, "this needs a human call" has to be something they can say and something
that reliably reaches one.
