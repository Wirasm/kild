# Vocabulary

The canonical nouns and verbs. One word per concept, one concept per word.

kild has exactly two consumers — our CLI and our UI (helm) — so names are changed outright.
No aliases, no deprecation window, no compatibility shims. If a word here is wrong, we change
it everywhere in one commit.

## Nouns

| Word | What it is | Not to be confused with |
|---|---|---|
| **kild** | A git worktree and the agents working in it. The unit of isolated, parallel work. | The product is also "kild"; a unit is "a kild". |
| **agent** | The sole inhabitant: one running agent process. Addressable, holds context, can spawn peers. | Not a "worker" — there is no rank. |
| **persona** | A markdown file that shapes what an agent is. Lives in `.pi/agents/` or `.claude/agents/`. | Not the running agent. The *dir* is called `agents/` by upstream convention — theirs, not ours. |
| **handle** | An agent's `@name`, addressable within its kild. | Not its `id`. |
| **id** | An agent's engine-assigned machine identity, globally unique. | Not its `handle`, not its pi session. |
| **message** | One directed communication from an agent to named recipients. | Never a "post" — see below. |
| **inbox** | Unread messages waiting for an agent to pull them. | Only attached agents pull; owned agents are pushed to. |
| **session** | pi's conversation — `piSessionId`, `piSessionFile`, the `pi --session` resume handle. | pi's word for pi's concept. It never means "a running agent". |
| **honryo** | An agent wearing human-authority intelligence. A PRP concept the engine cannot see. | Not an engine flag, not a privileged type. |
| **seat** | Hard authority: spawn, stop, kill, guardrails. Held by the human operating the engine. | Not delegable at the root — see DEMOLITION.md. |

## Verbs

| Verb | Does |
|---|---|
| **new** | Create a kild — fork a worktree from a base branch. |
| **spawn** | Create an owned agent inside a kild. |
| **attach** | Register an agent kild does *not* own (an external harness) so it is addressable. |
| **send** | Deliver a message to named recipients' inboxes. |
| **drain** | Destructively read an inbox. Accurate: the messages are consumed. |
| **stop** | Halt an agent or a whole kild. |
| **land** | Merge a kild's branch toward its base. |

## The two axes

An agent sits on exactly one **mechanism** axis and wears exactly one **intelligence** label.

- **Mechanism — `ownership`:** `owned` (kild runs the process; delivery is a push to stdin) or
  `attached` (an external harness; delivery is a queue it pulls). This is the only axis the
  engine knows.
- **Intelligence — `persona`:** whatever PRP hands it. The engine stores the string and never
  interprets it. "honryo" and "reviewer" are personas, not types.

## Words that are dead

| Dead | Why | Now |
|---|---|---|
| `room` | A social construct with its own lifecycle; the worktree is the real boundary. | `kild` |
| `worker` | Rank vocabulary — a worker implies a lead and an operator above it. Also imports the job-queue model: an interchangeable executor pulled from a pool. Agents are none of those things. | `agent` |
| `participant` | Membership in a room. Membership is just "in this tree". | `agent` |
| `post` | You post to a board; you send to a person. Broadcast vocabulary in a directed-messaging system. | `send` / `message` |
| `lead` | `participants[0]` was never a mechanism fact. | *(nothing — no rank)* |
| `operator` | A privileged session type the engine granted tools to. | *(nothing — clients drive the API)* |
| `brain` | A shipped orchestrator persona. kild ships no intelligence. | *(PRP's honryo)* |
| `<role>` | A third synonym for persona, while `KILD_ROLE` meant process role. | `<persona>` |
| `session` (kild-level) | Meant "a running agent" while `piSessionId` meant pi's conversation — one word, two things. | `agent` |

## Collisions we deliberately resolved

**`agent` meant three things.** The CLI's `kild agent ls` listed *personas*, the REST route for
the same data was `/api/personas`, and the code called it `listAgents()`. Meanwhile the running
thing had no stable name (worker / participant / session). Now: **persona** is the file,
**agent** is the process.

**`role` meant two things.** `withRole()` wrapped a persona in `<role>` tags while `KILD_ROLE`
named the process role. Now `<persona>` wraps personas and `KILD_ROLE=engine|agent` is the only
use of "role".

**`session` meant two things.** Now it means only pi's conversation.

## Naming rules

1. **One word per concept.** If two words mean the same thing, one is wrong.
2. **No rank words.** Nothing in the engine may imply a hierarchy — that is intelligence.
3. **Say what it is, not what it does for us.** An `inbox` holds messages; it is not a
   "notification channel".
4. **Verbs name the effect.** `drain` is destructive and says so; `send` names a recipient by
   construction, which is why the "who is this addressed to?" bug cannot be written.
5. **Upstream words stay upstream's.** `session` is pi's. `worktree`, `base`, `branch` are git's.
   We do not re-coin them.
