---
name: pipeline-docs
description: Run the full documentation pipeline for a single engine module: document → map-dependencies → validate in a single context window. Lighter-weight than /ralph. Use for documenting one engine at a time. Triggers on "/pipeline", "/pipeline [module]", "full pipeline", "document and connect".
user-invocable: true
allowed-tools: Read, Write, Edit, Grep, Glob
context: fork
---

## EXECUTE NOW

**Target: $ARGUMENTS**

- If target is a module: run full pipeline for that module
- If target is empty: ask which module to process

**Execute in order:**

1. **Document phase** — extract all engine specs from the module
2. **Map-dependencies phase** — connect each created spec to related specs
3. **Validate phase** — verify quality for each created spec
4. Report what was created, connected, and validated

**START NOW.**

---

# Pipeline-Docs

Single-context full pipeline: document → map-dependencies → validate.

## When to Use

- Documenting a single, self-contained module
- When you want all three phases in one context window
- When the module is small enough (< 500 lines) that all phases fit

Use /ralph instead when:
- Processing multiple modules
- Module is complex (needs fresh context per phase)
- You want independent phase verification

## Phase Sequence

### Phase 1: Document
- Read entire module
- Extract all meaningful content per extraction categories
- Create engine spec notes in `engine-specs/`
- Note: proceed to Phase 2 immediately (no user prompt)

### Phase 2: Map Dependencies
- For each created spec, find connections
- Update relevant system maps
- Add inline wiki-links
- Note: proceed to Phase 3 immediately

### Phase 3: Validate
- For each created spec, run validate checks
- Apply auto-fixes if needed
- Report pass/fail status

## Output

```
## Pipeline Complete: [module]

Phase 1 - Document: N specs created
Phase 2 - Map Dependencies: N connections added, N system maps updated
Phase 3 - Validate: N passed, N warnings, N failed

### Engine Specs Created
- [[spec name]] — [description]

### Issues
- [any warnings or failures]
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
