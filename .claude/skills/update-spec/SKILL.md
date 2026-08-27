---
name: update-spec
description: Revisit existing engine specs when the underlying module changes, blueprints are updated, or architectural decisions evolve. Keeps specs current with actual implementation. Use when a module is updated, when a v1.1 spec supersedes v1.0, or when engine specs show drift from reality. Triggers on "/update-spec", "/update-spec [spec]", "update engine spec", "spec is stale", "module changed".
user-invocable: true
allowed-tools: Read, Write, Edit, Grep, Glob
context: fork
---

## Runtime Configuration (Step 0)

Read `ops/derivation-manifest.md` for vocabulary. Key: `vocabulary.cmd_verify` → `/validate`.

## EXECUTE NOW

**Target: $ARGUMENTS**

- If target is a spec name: update that engine spec
- If target is "all": check all specs for staleness
- If target is empty: ask which spec to update

**Execute these steps:**

1. Read the current engine spec in `engine-specs/`
2. Read the current Python module or blueprint it documents
3. Identify what has changed: new methods, new dependencies, changed contracts, deprecated features
4. Update the spec note to reflect current reality
5. If changes are significant: update connected system maps
6. Update `updated_at` field in frontmatter
7. Flag if related specs (callers, dependencies) also need updating

**START NOW.**

---

# Update-Spec

Keep engine specs synchronized with actual implementation.

## When to Update

| Trigger | Action |
|---------|--------|
| Module gained new public method | Add interface spec note or update existing |
| Module now depends on new engine | Update dependency section, add wiki-link |
| Blueprint version bumped (v1.0 → v1.1) | Update description, add new constraint notes |
| Method signature changed | Update data contract notes |
| Engine deprecated | Change `status: deprecated`, move to deprecated/ |
| Performance characteristic discovered | Add note to spec body |

## Staleness Detection

Run `/update-spec --check-all` to scan for specs that may be stale:

```bash
# Find specs with updated_at > 30 days ago
grep -r "updated_at:" engine-specs/*.md | \
  awk -F': ' '{print $2, $1}' | sort | head -20
```

Compare spec's `updated_at` against the module's actual modification date.

## Update Process

1. **Read spec** — understand what it currently claims
2. **Read module** — understand what it currently does
3. **Diff the claims** — what is new, changed, or removed?
4. **Update description** — if the core claim changed
5. **Update body** — add new information, strike/remove outdated claims
6. **Update metadata** — `updated_at`, `version` if applicable
7. **Propagate** — check if callers or dependents also need updating

## Output Format

```
## Update-Spec Complete: [engine spec]

### Changes Made
- [Field/section]: [what changed]
- [Field/section]: [what changed]

### Propagation Needed
- [[caller spec]] — [why it may need updating]

### Next Step
/validate [spec]
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
