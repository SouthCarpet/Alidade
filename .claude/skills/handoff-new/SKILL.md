---
name: handoff-new
description: "Start-of-session handoff consumer. Reads the execution handoff JSON from a previous session and resumes work with full context continuity. Use when starting a new session after /handoff-old, when the user says 'continue from handoff', 'resume', 'pick up where we left off', 'load handoff', or 'continue last session'. Also triggers when handoff/execution-handoff.json exists and the user asks to continue previous work."
user-invocable: true
---

# /handoff-new — Session Handoff Consumer

Resume work from a previous session's handoff. Read the execution handoff, verify the environment matches, and start executing immediately.

## Trigger

- `/handoff-new` — load and resume from `handoff/execution-handoff.json`
- `/handoff-new path/to/handoff.json` — load from a custom path

## Process

### Step 1: Load and Validate the Handoff

Read `handoff/execution-handoff.json` (or the provided path). If the file doesn't exist or is invalid JSON, stop and tell the user — don't guess or improvise.

Validate the handoff has the required fields:
- `objective`, `summary`, `current_state`, `ordered_execution_plan`

If `handoff_metadata` exists (newer format), use it to verify environment:

```bash
# Compare current branch with handoff branch
git branch --show-current

# Check for unexpected changes since handoff
git log --oneline -5

# Verify uncommitted changes haven't been lost
git status --short
```

**Branch mismatch warning**: If the current branch differs from `handoff_metadata.branch`, warn the user before proceeding. The handoff may reference work on a different branch.

**Staleness check**: If `handoff_metadata.created_at` is more than 48 hours old, warn: "This handoff is N days old. The codebase may have changed since then. Shall I verify the state before executing, or proceed as-is?"

### Step 2: Restate Key Context

Briefly restate (2-4 lines total, not a wall of text):
1. **Objective** — what we're building toward
2. **First executable step** — what to do right now
3. **Key constraints** — things that must not break

This serves as a sanity check — the user can correct course here before execution begins.

### Step 3: Verify State Before Executing

Before touching any code, verify the handoff's assumptions still hold. This prevents the most common handoff failure: the codebase changed between sessions.

For each file listed in `modified_files` (if present) or referenced in `ordered_execution_plan`:
- Verify the file exists
- If the handoff says something was "done", spot-check that it's actually there (e.g., if "added retry_steps() function", grep for it)

For items marked as `done` in `current_state`:
- Do a quick verification on 2-3 key items (not all — just enough to confirm the handoff is trustworthy)

If something doesn't match, flag it immediately rather than discovering it mid-execution.

### Step 4: Execute the Plan

Follow `ordered_execution_plan` sequentially:

1. **Start with the first non-completed step.** Cross-reference with `progress_log` — if a step is marked "done" there and verification confirms it, skip it.

2. **Respect dependencies.** If `depends_on` lists step IDs, verify those steps are actually complete before starting.

3. **Read before writing.** For each step, read the files listed in `files` (or the files you'll modify) before making changes. The handoff captures intent, but the code is the source of truth.

4. **Apply `next_session_rules`.** These are constraints from the previous session — treat them like CLAUDE.md rules for this session.

5. **Preserve `architecture_decisions`.** Every decision marked `must_preserve: true` is a hard constraint. Don't change the approach even if you think of something "better" — the previous session made that choice for a reason documented in the `reason` field.

6. **Learn from `similar_bugs_or_earlier_fixes`.** Before implementing, scan this list. If you're about to do something similar to a previous bug, take the learning into account.

### Step 5: Update Progress

At meaningful milestones (after completing each step or hitting a blocker), update the handoff file to reflect current state:

```python
# Update progress in the handoff
import json
with open('handoff/execution-handoff.json', 'r') as f:
    handoff = json.load(f)

# Move items between states as work progresses
# Update progress_log with new entries
# Mark completed steps

with open('handoff/execution-handoff.json', 'w') as f:
    json.dump(handoff, f, indent=2)
```

This ensures that if *this* session also gets interrupted, the next `/handoff-new` will have accurate state.

### Step 6: Handle Blockers

If you hit a blocker that can't be resolved from the handoff or codebase:

1. **Check `important_context`** — the answer might be there
2. **Check `similar_bugs_or_earlier_fixes`** — similar issue might have been solved before
3. **Check `risks`** — the previous session may have anticipated this
4. **Only then ask the user** — with a specific question, not a broad "what should I do?"

Do NOT restart broad brainstorming. Do NOT re-explore the codebase from scratch. The handoff exists precisely to avoid this.

## Rules

- The handoff is the source of truth for *intent and decisions*. The code is the source of truth for *current state*.
- Do not re-analyze or re-plan what the handoff already covers. Execute.
- Do not ask broad planning questions if the handoff is sufficient.
- Work sequentially through the execution plan.
- Minimize rework — verify state before writing.
- If the handoff has gaps or ambiguity, resolve from the codebase first, user second.
- Keep progress continuity — update the handoff file at milestones.
- Do not touch files or systems the handoff marks as complete unless verification shows they're broken.

## Output

After loading, report:
```
Handoff loaded from [path]
- Objective: [one-line objective]
- Branch: [current] (handoff expects: [expected])
- Done: N | Partial: N | Remaining: N
- First step: [title of first executable step]
- Constraints: [count] architecture decisions, [count] session rules

Proceeding with step [N]: [title]
```

Then begin execution immediately.


<!-- multi-vault-block:v1 -->
## Multi-vault scope (extension — default unchanged)

Default scope is THIS project's vault — original behavior. The flags
below are opt-in:

| Flag                | Behavior                                              |
|---------------------|-------------------------------------------------------|
| (default)           | THIS vault / project — original behavior              |
| `--all`             | Iterate every enabled vault from the central registry |
| `--vault <name>`    | One named sibling vault from the registry             |

Adapter (single source of truth — `A:/projects-vault/vault/ops/registry.yaml`):
```bash
py A:/projects-vault/scripts/multi_vault.py list --include-self --json
py A:/projects-vault/scripts/multi_vault.py for-each --cwd-mode project -- <cmd>
```

Each per-vault run uses the same procedure as the single-vault default;
multi-vault aggregation happens at the end. **No existing single-vault
caller's behavior changes.**
<!-- /multi-vault-block:v1 -->
