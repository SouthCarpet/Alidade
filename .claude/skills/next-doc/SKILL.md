---
name: next-doc
description: Intelligent next-action recommendations for vault work. Analyzes queue, orphans, stale specs, and undocumented modules to suggest the highest-value next step. Triggers on "/next", "what should I do next", "next documentation task", "prioritize".
user-invocable: true
allowed-tools: Read, Glob, Grep, Bash
context: fork
---

## EXECUTE NOW

**Target: $ARGUMENTS**

- Empty: show recommended next action
- "queue": show full task queue status
- "stale": show specs that need updating

**START NOW.**

---

# Next-Doc

Get intelligent recommendations for what to work on next.

## Decision Logic

Check in priority order:

1. **Failing validate specs** — any spec with known FAIL issues
   - Action: `/validate [spec]` then fix

2. **In-progress tasks** — tasks marked in_progress in queue
   - Action: continue the interrupted pipeline

3. **Undocumented core engines** — modules with no engine specs yet
   - Priority: production_pipeline, content_strategy_orchestrator, scriptwriter
   - Action: `/pipeline [module]`

4. **Orphan specs** — specs with 0 incoming links after 7+ days
   - Action: `/map-dependencies [spec]`

5. **Stale specs** — specs where module was modified more recently than spec
   - Action: `/update-spec [spec]`

6. **Pending staging** — files in `staging/` not yet processed
   - Action: `/document [staged file]`

## Stale Detection

```bash
# Find specs older than their source modules
for spec in engine-specs/*.md; do
  module=$(grep "^module:" "$spec" | cut -d' ' -f2)
  if [ -f "$module" ]; then
    spec_time=$(date -r "$spec" +%s)
    mod_time=$(date -r "$module" +%s)
    if [ $mod_time -gt $spec_time ]; then
      echo "STALE: $spec (module newer by $((mod_time - spec_time))s)"
    fi
  fi
done
```

## Output

```
## Next Action Recommendation

### Recommended: [action]
[Why this is highest priority]

Command: [exact command to run]

### Other Options
1. [second priority] — [reason]
2. [third priority] — [reason]

### Queue Summary
  Pending: N tasks
  In progress: N tasks
  Completed: N tasks
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
