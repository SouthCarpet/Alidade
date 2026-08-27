# handoff — Claude Code Session Continuity Skill

A Claude Code skill that preserves full context between sessions. End a session with `/handoff save` and pick up exactly where you left off with `/handoff load` — no re-exploration, no lost context.

## What it does

`/handoff save` scans your current session at the end: it gathers git state, reads the conversation for completed work, decisions, bugs, and constraints, and writes a compact JSON file (`handoff/execution-handoff.json`) optimized for the next session to execute from.

`/handoff load` reads that JSON at the start of the next session, validates the environment, restates key context as a sanity check, and begins executing the plan immediately.

## Install

Add this skill to Claude Code via the marketplace:

1. Open **Claude Code**
2. Go to **Settings → Plugins/Skills → Add from GitHub**
3. Enter the repo URL:
   ```
   https://github.com/SouthCarpet/handoff
   ```
4. Both `/handoff save` and `/handoff load` will be available immediately

## Usage

### End of session
```
/handoff save
```
Creates `handoff/execution-handoff.json` in your project root.

Preview without saving:
```
/handoff save --dry-run
```

### Start of next session
```
/handoff load
```
Loads from `handoff/execution-handoff.json`.

Load from a custom path:
```
/handoff load path/to/my-handoff.json
```

## Commands reference

| Command | When to use |
|---|---|
| `/handoff save` | Ending a session, wrapping up, preparing for next session |
| `/handoff save --dry-run` | Preview the handoff JSON without saving |
| `/handoff load` | Starting a new session, continuing previous work |
| `/handoff load <path>` | Load handoff from a custom file path |

## How it works

The handoff JSON captures everything the next session needs:

| Field | What it contains |
|---|---|
| `objective` | The real end goal in one sentence |
| `current_state` | Done / partial / not started items |
| `ordered_execution_plan` | Sequenced steps with file paths and notes |
| `architecture_decisions` | Decisions already made that must be preserved |
| `next_session_rules` | Execution constraints for the next session |
| `starter_prompt` | A self-contained prompt to start immediately |
| `risks` | Known risks with prevention strategies |
| `modified_files` | Every file changed, for state verification |

## Tips

- Commit before running `/handoff save` — the handoff captures git state as context
- If the handoff is more than 48 hours old, `/handoff load` will warn you before proceeding
- The `starter_prompt` field in the JSON is designed to be copy-pasted as your opening message in a new session
- Add `handoff/` to your `.gitignore` if you don't want to commit the JSON to your repo
