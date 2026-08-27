---
name: handoff
description: "Session handoff skill with two commands. /handoff save creates a compact execution-optimized JSON handoff at end of session — use when ending a session, wrapping up work, or the user says 'handoff', 'save progress', 'session end', 'wrap up', 'create handoff', or 'prepare for next session'. /handoff load resumes from that handoff at the start of a new session — use when starting a new session after /handoff save, when the user says 'continue from handoff', 'resume', 'pick up where we left off', 'load handoff', or 'continue last session'. Always use /handoff save instead of manually writing handoff JSON."
user-invocable: true
---

# /handoff — Session Handoff

Two commands for seamless session continuity:

- `/handoff save` — End of session: generate a compact JSON handoff
- `/handoff save --dry-run` — Preview the handoff without saving
- `/handoff load` — Start of session: resume from `handoff/execution-handoff.json`
- `/handoff load path/to/handoff.json` — Load from a custom path

---

## /handoff save — Session Handoff Generator

Create a strict execution handoff as valid JSON. The goal is a compact, high-signal document that lets a fresh Claude Code session continue this work with minimal token waste and zero re-exploration.

### Step 1: Gather State Automatically

Before writing anything, gather the actual state. Run these in parallel:

```bash
# Git state
git branch --show-current
git status --short
git stash list
git log --oneline -10
git diff --stat HEAD
```

This data feeds directly into the handoff — the next session needs to know what branch it's on, whether there are uncommitted changes, and what the recent history looks like.

### Step 2: Review Conversation Context

Scan the current conversation for:
- **Completed work** — what was actually done (not just discussed)
- **Partial work** — things started but not finished
- **Architecture decisions** — choices made and why
- **Bugs encountered** — what broke and how it was fixed
- **Constraints discovered** — things that must not change
- **User corrections** — feedback that shaped the approach
- **Files modified** — every file that was changed or created

Separate signal from noise. Brainstorming ideas that were rejected, repeated attempts at the same fix, exploratory reads that led nowhere — these are noise. Only keep what the next session needs to execute correctly.

### Step 3: Build the Handoff JSON

Use exactly this schema:

```json
{
  "handoff_metadata": {
    "created_at": "ISO 8601 timestamp",
    "branch": "current git branch",
    "uncommitted_changes": ["list of files with uncommitted changes"],
    "last_commit": "short hash + message of last commit"
  },
  "objective": "string — the real end goal in one sentence",
  "summary": "string — short high-signal summary of current situation, 2-3 sentences max",
  "current_state": {
    "done": ["string — completed items, be specific about what was done and where"],
    "partial": ["string — in-progress items with clear description of what remains"],
    "not_started": ["string — items that still need to be done"]
  },
  "modified_files": [
    {
      "file": "relative/path/to/file.py",
      "changes": "brief description of what changed"
    }
  ],
  "architecture_decisions": [
    {
      "decision": "string — what was decided",
      "reason": "string — why this approach was chosen",
      "must_preserve": true
    }
  ],
  "important_context": [
    "string — only context needed for correct execution, not general knowledge"
  ],
  "similar_bugs_or_earlier_fixes": [
    {
      "issue": "string — what went wrong",
      "fix_or_learning": "string — how it was resolved or what was learned"
    }
  ],
  "ordered_execution_plan": [
    {
      "step_id": 1,
      "title": "string — short action title",
      "goal": "string — what this step achieves",
      "depends_on": [],
      "files": ["list of files this step touches"],
      "notes": "string — implementation hints, gotchas, or constraints"
    }
  ],
  "risks": [
    {
      "risk": "string — what could go wrong",
      "impact": "string — consequences",
      "prevention": "string — how to avoid it"
    }
  ],
  "progress_log": [
    {
      "status": "done | partial | pending",
      "item": "string — what was done",
      "notes": "string — relevant details"
    }
  ],
  "next_session_rules": [
    "string — execution rules for the next session"
  ],
  "starter_prompt": "string — one concise prompt for the next session that tells Claude exactly how to continue"
}
```

### Step 4: Quality Checks

Before saving, verify:

1. **Valid JSON** — parse it with `python -c "import json; json.load(open('...'))"` after saving
2. **No noise** — remove brainstorming ideas, dead ends, repeated thoughts, exploration that led nowhere
3. **Actionable plan** — every step in `ordered_execution_plan` should be directly executable, not analytical
4. **No duplicates** — merge overlapping tasks
5. **Correct ordering** — tasks ordered to minimize rework; dependencies declared correctly
6. **Files exist** — verify key files mentioned in `modified_files` actually exist
7. **Starter prompt is complete** — it should contain enough context that the next session can start working immediately without reading the full handoff first

### Step 5: Save

Create the handoff directory if needed and save:

```bash
mkdir -p handoff
```

Save to `handoff/execution-handoff.json`.

Validate after saving:
```bash
python -c "import json; data=json.load(open('handoff/execution-handoff.json')); print(f'Valid JSON: {len(data[\"ordered_execution_plan\"])} steps, {len(data[\"current_state\"][\"done\"])} done, {len(data[\"current_state\"][\"not_started\"])} remaining')"
```

### Field Rules

- **objective** = the real end goal in one sentence. Not "continue working on X" but what the final deliverable is.
- **summary** = high-signal summary of where things stand right now. What works, what doesn't, what's next.
- **current_state** = split clearly into done / partial / not_started. Be specific — "added retry_steps() to production_pipeline.py" not "worked on pipeline".
- **modified_files** = every file changed in this session with a brief description. The next session uses this to quickly verify state.
- **architecture_decisions** = only decisions already made that constrain future work. Include the reason — the next session needs to understand *why* to handle edge cases correctly.
- **important_context** = only context needed for correct execution. If the next session could figure it out by reading the code, leave it out.
- **similar_bugs_or_earlier_fixes** = mistakes and learnings that prevent the next session from repeating them.
- **ordered_execution_plan** = strict execution order. Each step has the files it touches and any implementation hints. Dependencies are explicit.
- **risks** = only real risks worth preserving. "Code might have bugs" is not a risk. "Step dependency logic could break full-pipeline runs" is.
- **next_session_rules** = execution constraints. "Read X before changing Y", "don't touch Z", "test after each step".
- **starter_prompt** = a self-contained prompt that includes: what to do first, key constraints, and where to find details. The next session should be able to start from this alone.

### Rules

- Output ONLY valid JSON to the file. No markdown, no code fences, no commentary in the file.
- Work sequentially through the gathering steps, not in parallel.
- Prefer execution-optimized handoffs, not analytical ones. The next session should execute, not re-analyze.
- If something is not needed for the next session, leave it out.
- If a dependency is unknown, use an empty array.
- If a section has no items, output an empty array.
- Merge overlapping tasks — don't create 5 steps for what could be 2.
- The `starter_prompt` is the most important field. Spend extra effort making it precise and complete.

### Output

After saving, report to the user:
```
Handoff saved to handoff/execution-handoff.json
- Objective: [one-line objective]
- Done: N items | Partial: N items | Remaining: N items
- Execution plan: N steps
- Branch: [branch name] | Uncommitted: [count] files

Next session: paste the starter_prompt or use /handoff load
```

---

## /handoff load — Session Handoff Consumer

Resume work from a previous session's handoff. Read the execution handoff, verify the environment matches, and start executing immediately.

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

This ensures that if *this* session also gets interrupted, the next `/handoff load` will have accurate state.

### Step 6: Handle Blockers

If you hit a blocker that can't be resolved from the handoff or codebase:

1. **Check `important_context`** — the answer might be there
2. **Check `similar_bugs_or_earlier_fixes`** — similar issue might have been solved before
3. **Check `risks`** — the previous session may have anticipated this
4. **Only then ask the user** — with a specific question, not a broad "what should I do?"

Do NOT restart broad brainstorming. Do NOT re-explore the codebase from scratch. The handoff exists precisely to avoid this.

### Rules

- The handoff is the source of truth for *intent and decisions*. The code is the source of truth for *current state*.
- Do not re-analyze or re-plan what the handoff already covers. Execute.
- Do not ask broad planning questions if the handoff is sufficient.
- Work sequentially through the execution plan.
- Minimize rework — verify state before writing.
- If the handoff has gaps or ambiguity, resolve from the codebase first, user second.
- Keep progress continuity — update the handoff file at milestones.
- Do not touch files or systems the handoff marks as complete unless verification shows they're broken.

### Output

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
