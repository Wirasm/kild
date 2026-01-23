---
description: Triage GitHub issues by applying type, effort, priority, and area labels
argument-hint: "[unlabeled|all|N|N-M]"
model: sonnet
allowed-tools: Bash, Read
---

# Triage GitHub Issues

You are tasked with triaging GitHub issues for the Shards project by applying appropriate labels.

**Before you begin:** Take a moment to understand that good triage requires careful consideration. Each label decision affects how work gets prioritized and filtered. Read each issue thoroughly and think through your reasoning before applying labels.

## Project Philosophy (Critical Context)

This project has a core principle: **"No Silent Failures"**

From the project's CLAUDE.md:
> "This is a developer tool. Developers need to know when something fails. Never swallow errors, never hide failures behind fallbacks without logging, never leave things 'behind the curtain'. If config is wrong, say so. If an operation fails, surface it. Explicit failure is better than silent misbehavior."

**This means:** Any issue describing behavior where errors are hidden, failures are swallowed, fallbacks happen silently, or users aren't informed of problems is a **bug**, not a feature request. The current behavior violates the project's design principles.

## Decision-Making Principles

**Signal over noise:** Every label should add meaningful information. Don't over-label. If unsure between two options, pick the one that helps future filtering most.

**Focus on "why" not "what":** When assessing priority, consider *why* this matters to users, not just what the code change involves.

**Effort is about scope and focus:**
- `effort/low` - Single file or function, one responsibility, isolated change
- `effort/medium` - Few files, one domain or module, some coordination needed
- `effort/high` - Cross-cutting changes, multiple domains, requires design decisions

Think carefully: Does implementing this require touching multiple subsystems? Will it need design discussions? Could a focused PR solve it, or does it need architectural consideration?

**Priority is about urgency and impact:**
- `P0` - Critical, do first. Blocking issues, broken core functionality, violations of core principles
- `P1` - High priority, address soon. Significant bugs, important features, user-facing problems
- `P2` - Backlog, when time permits. Useful but not urgent
- `P3` - Nice to have, consider closing if stale. Minor polish or unlikely to implement

Ask yourself: What happens if this isn't fixed for 6 months? Does it block users? Does it violate the project's principles? Does it cause confusion or frustration?

**Area labels match logging domains:** This project uses structured logging with `{layer}.{domain}` convention. Area labels should mirror this for grep-ability across logs and issues. Consider all domains an issue touches.

## Steps to Follow

1. **Fetch available labels:**
   ```bash
   gh label list --json name,description
   ```
   - Review the label descriptions to understand their intended use
   - Labels are organized into categories: type, effort, priority, and area

2. **Determine which issues to triage based on $ARGUMENTS:**

   | Argument | Behavior |
   |----------|----------|
   | (empty) | Only unlabeled issues (default) |
   | `unlabeled` | Only issues without any labels |
   | `all` | All open issues |
   | `N` | Specific issue (e.g., `67`) |
   | `N-M` | Range of issues inclusive (e.g., `60-67`) |

   Fetch issues accordingly:
   ```bash
   # Unlabeled issues only (default)
   gh issue list --state open --json number,title,body,labels --limit 100
   # Then filter to only those with empty labels array

   # All open issues
   gh issue list --state open --json number,title,body,labels --limit 100

   # Specific issue
   gh issue view {number} --json number,title,body,labels

   # Range - fetch all and filter by number
   gh issue list --state open --json number,title,body,labels --limit 100
   # Then filter to numbers within range
   ```

3. **For each issue, think through the classification:**

   Before applying labels, reason through each decision:

   **Type label (pick one primary):**

   First, ask: "Is the current behavior correct according to the project's principles?"

   - Is it reporting broken behavior or principle violations? → `bug`
     - Silent failures, swallowed errors, hidden fallbacks = **bug**
     - Unexpected behavior that surprises users = **bug**
     - Things that "work" but violate project philosophy = **bug**
   - Is it a planned new capability? → `feature`
   - Is it an external suggestion needing review? → `feature-request`
   - Is it about documentation? → `docs`
   - Is it maintenance/refactoring/CI? → `chore`
   - Is it a question? → `question`
   - Does it involve security? → `security`
   - Is it specifically about performance? → `performance`
   - Does it introduce breaking changes? → `breaking`

   **Effort label (pick one):**

   Think through the implementation:
   - How many files/modules does this touch?
   - Is it isolated or cross-cutting?
   - Does it require design decisions or just mechanical changes?
   - Will it need new abstractions or just modifications to existing code?

   **Priority label (pick one):**

   Consider the user impact:
   - How urgent is this for users?
   - Does it block other work or common workflows?
   - What's the impact of not fixing it?
   - Does it violate core project principles?

   **Area labels (one or more):**

   Map to the codebase structure:
   - Which modules/domains does this affect?
   - Consider both the symptom location AND the fix location
   - An issue might manifest in CLI but require core changes

4. **Apply labels with reasoning:**

   For each issue, briefly state your reasoning, then apply:
   ```bash
   gh issue edit {number} --add-label "type,effort/level,P#,area.domain"
   ```
   - Skip issues that already have complete labeling (type + effort + priority + area)
   - For partially labeled issues, only add missing label categories

5. **Generate summary output:**

   After processing all issues, output a summary:

   ```
   ## Triage Summary

   | Issue | Title | Labels Applied | Reasoning |
   |-------|-------|----------------|-----------|
   | #67 | PID file retry... | bug, effort/low, P1, core.config | Config defaults cause silent failure |

   **Totals:**
   - Issues triaged: X
   - Already labeled (skipped): Y
   - By priority: P0(n), P1(n), P2(n), P3(n)
   ```

## Important Notes

- **Think before labeling** - Don't rush through issues. Each one deserves consideration.
- **Silent failures are bugs** - If something fails silently or hides errors, it's broken behavior.
- **Don't hardcode labels** - Always fetch current labels with `gh label list` as they may change.
- **Respect existing labels** - Don't remove labels, only add missing ones.
- **One type label** - Issues should have exactly one primary type.
- **Area labels can stack** - An issue can touch multiple areas (e.g., `core.config` + `cli`).
- **When uncertain, ask** - If an issue is ambiguous, ask the user rather than guessing.
- **Check issue body** - The title alone often isn't enough context; read the full description.
