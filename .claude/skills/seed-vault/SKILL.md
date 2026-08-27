---
name: seed-vault
description: Bootstrap the knowledge vault by systematically documenting all Python engine modules and blueprints. Run once to seed initial engine specs from the codebase. Processes each module through the full pipeline: document → map-dependencies → validate. Triggers on "/seed", "seed the vault", "document all engines", "bootstrap knowledge base".
user-invocable: true
allowed-tools: Read, Write, Glob, Bash
context: fork
---

## EXECUTE NOW

**Target: $ARGUMENTS**

- If target is empty: seed all Python modules in the project root
- If target is a directory: seed all .py files in that directory
- If target is a module name: seed just that module

**Execute these steps:**

1. Discover all Python modules: `*.py` in project root
2. Discover all blueprints: `blueprints/*.json`
3. For each source, create a task entry in `ops/queue/`
4. Process each source through /document
5. After all documents created, run /map-dependencies for each
6. Report what was seeded

**START NOW.**

---

# Seed

Bootstrap the vault from zero by processing all engine modules.

## Seeding Order

Process in dependency order (lowest-level engines first):

1. **Data layer**: `project_registry.py`, `niche_data.py`
2. **Model routing**: `model_router.py`
3. **Generation engines**: `scriptwriter.py`, `tts_engine.py`, `image_engine.py`, `video_engine.py`
4. **Orchestration**: `production_pipeline.py`
5. **Intelligence**: `content_strategy_orchestrator.py`, `gemini_trends.py`
6. **Packaging**: `scene_builder.py`, `thumbnail_maker.py`
7. **Quality**: `channel_memory.py`, `monetization_compliance.py`
8. **Application**: `app.py`
9. **Blueprints**: all `blueprints/*.json`

## Progress Tracking

Create `ops/tasks.md` entry for the seeding run:

```markdown
## Seed Run: [YYYY-MM-DD]
Status: in_progress
Sources: N total
Completed: N
Remaining: [list]
```

## Output

```
## Seed Complete

### Sources Processed
- [module name]: N engine specs created
- [module name]: N engine specs created

### Total
- Engine specs created: N
- System maps updated: N

### Next Step
/map-dependencies [first spec] — begin connection mapping
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
