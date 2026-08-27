---
name: handoff-old
description: "End-of-session handoff generator. Creates a compact, execution-optimized JSON handoff for the next Claude Code session. Use when ending a session, wrapping up work, handing off to a new session, or when the user says 'handoff', 'save progress', 'session end', 'wrap up', 'create handoff', or 'prepare for next session'. Always use this instead of manually writing handoff JSON."
user-invocable: true
---

# /handoff-old — Session Handoff Generator

Create a strict execution handoff as valid JSON. The goal is a compact, high-signal document that lets a fresh Claude Code session continue this work with minimal token waste and zero re-exploration.

## Trigger

- `/handoff-old` — generate full handoff from current session
- `/handoff-old --dry-run` — show handoff preview without saving

## Process

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

## Field Rules

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

## Rules

- Output ONLY valid JSON to the file. No markdown, no code fences, no commentary in the file.
- Work sequentially through the gathering steps, not in parallel.
- Prefer execution-optimized handoffs, not analytical ones. The next session should execute, not re-analyze.
- If something is not needed for the next session, leave it out.
- If a dependency is unknown, use an empty array.
- If a section has no items, output an empty array.
- Merge overlapping tasks — don't create 5 steps for what could be 2.
- The `starter_prompt` is the most important field. Spend extra effort making it precise and complete.

## Output

After saving, report to the user:
```
Handoff saved to handoff/execution-handoff.json
- Objective: [one-line objective]
- Done: N items | Partial: N items | Remaining: N items
- Execution plan: N steps
- Branch: [branch name] | Uncommitted: [count] files

Next session: paste the starter_prompt or use /handoff-new
```


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
