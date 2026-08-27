---
name: validate-spec
description: Validate engine spec schema compliance, description quality, and graph health. Use as a quality gate after creating engine specs or as periodic maintenance. Combined validation — recite (description quality) + schema check + link health. Triggers on "/validate", "/validate [spec]", "check spec quality", "validate engine spec".
user-invocable: true
allowed-tools: Read, Write, Edit, Grep, Glob
context: fork
---

## Runtime Configuration (Step 0 — before any processing)

Read `ops/derivation-manifest.md` for vocabulary and `ops/config.yaml` for processing depth.

## EXECUTE NOW

**Target: $ARGUMENTS**

- If target is a note name: validate that engine spec
- If target is "all" or "recent": validate recently created/modified specs
- If target is empty: ask which spec to validate

**Execute in order:**

### Step 1: RECITE (cold-read prediction test)

CRITICAL: Do NOT read the full note yet. Only read frontmatter.

1. Read ONLY title + description
2. Form prediction — what does this engine spec document? What are its interfaces? What are its dependencies?
3. Write prediction explicitly
4. Read full note content
5. Score prediction accuracy 1-5 (threshold: 3 = pass)

If score < 3: description needs rewriting.

### Step 2: VALIDATE (schema check)

Read `templates/engine-spec.md` for `_schema` block. Check:

**Required fields (FAIL if missing):**
- `description` — must exist and be non-empty
- `module` — must name the Python module or file
- `topics` — must reference at least one system map

**Enum fields (WARN if invalid):**
- `status`: active | deprecated | planned | experimental
- `maturity`: stable | beta | alpha
- `layer`: production | intelligence | quality | packaging | data

**Composability check:**
- Can you complete: "This engine spec documents that [title]"?
- Is the claim specific enough to be wrong?

### Step 3: REVIEW (health checks)

1. YAML frontmatter valid (has `---` delimiters, parses cleanly)
2. Description adds information beyond title
3. Engine spec appears in at least one system map's Core Engines section
4. At least 2 outgoing wiki links in spec body
5. All wiki links resolve to existing files (check body + frontmatter)

### Step 4: Apply fixes

Auto-fix if clear:
- Improved description if score < 3
- Missing `---` delimiters
- Trailing period on description

Do NOT auto-fix:
- Wrong enum values (requires understanding the engine's state)
- Missing connections (use /map-dependencies instead)

### Step 5: Report

```
=== VALIDATE: [engine spec] ===

RECITE:
  Prediction score: N/5
  Description: [pass/improved]

VALIDATE:
  Required fields: [PASS/FAIL]
  Enum fields: [PASS/WARN]
  Schema compliance: [PASS/WARN/FAIL]

REVIEW:
  Frontmatter: [PASS/FAIL]
  System map connection: [PASS/FAIL]
  Wiki links: N outgoing [PASS/WARN]
  Link resolution: [PASS/FAIL]

Overall: [PASS / WARN (N) / FAIL (N)]
===
```

## Pipeline Chaining

Validate is the final pipeline phase. After verification:
- **manual:** Output "Validated. Pipeline complete."
- **suggested:** Suggest marking task done in queue

If FAIL: keep task at `current_phase: "validate"` for re-run after fixes.


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
