---
name: health
description: Multi-vault aware health diagnostics. Default behavior is single-vault (this project) and delegates to `arscontexta:health` for the actual 8-category report. Pass `--all` to run diagnostics across every vault registered in `A:/projects-vault/vault/ops/registry.yaml`, then surface the consolidated FAIL/WARN/PASS picture. Triggers on "/health", "/health --all", "/health quick", "/health full", "/health three-space", "check vault health", "what needs fixing".
user-invocable: true
allowed-tools: Read, Grep, Glob, Bash, Write
context: fork
argument-hint: "[mode] [--all] — mode is one of: quick (default), full, three-space"
---

## EXECUTE NOW

**Target: $ARGUMENTS**

Argument parsing (FIRST step, before any disk I/O):

| Token | Meaning |
|-------|---------|
| empty / `quick` / `full` / `three-space` | Diagnostic mode (forwarded to arscontexta:health) |
| `--all`               | Run for EVERY enabled vault, not just this one |
| `--vault <name>`      | Run for one named vault only |
| `--dry-run`           | Print plan, no report written |

**Default is single-vault.** Without `--all`, this skill behaves identically to `arscontexta:health` on the current project. The wrapper exists so the same `/health` invocation extends cleanly to multi-vault, never the other way around (single-vault first is the regression-safe default).

**Execute these steps:**

1. **Parse mode and scope** from `$ARGUMENTS`.

2. **Resolve target vaults**:
   ```bash
   # Single (default):
   py scripts/vault/multi_vault.py current --json

   # All enabled vaults:
   py scripts/vault/multi_vault.py list --include-self --json

   # One named:
   py scripts/vault/multi_vault.py info <name> --json
   ```

3. **For each target vault**, run the diagnostic. Two execution modes:

   - **Single-vault (default)**: invoke `arscontexta:health` directly on the
     current project's `.vault/`. This keeps the original behavior intact
     for any caller that did not opt in to multi-vault.

   - **Multi-vault (`--all`)**: for every enabled entry from the registry:
     a. Read the vault's `root` and `process_dir` (if any).
     b. Run the 8-category diagnostic procedure described in
        `arscontexta:health` SKILL.md, but scoped to that vault's root.
     c. Write the per-vault report to `<vault_root>/ops/health/<YYYY-MM-DD>-report.md`.

4. **Aggregate** results into a single cross-vault summary:
   ```
   === MULTI-VAULT HEALTH SUMMARY ===
   Date: YYYY-MM-DD
   Scope: N vaults (5 registered, 4 enabled, 1 skipped)

   [pdf-editor]     7 PASS / 1 WARN / 0 FAIL
   [fitness-app]    6 PASS / 2 WARN / 0 FAIL
   [hair-app]       5 PASS / 2 WARN / 1 FAIL
   [youtube]       6 PASS / 1 WARN / 1 FAIL
   [projects-vault] (meta — skipped by default; pass --include-meta to scan)

   Cross-vault findings:
     - 4 dangling links across 2 vaults
     - 7 orphan notes total (3 > 7 days old)
     - 1 schema-version mismatch (youtube uses schema v1, others on v1)

   Top recommended actions:
     1. [hair-app]   fix dangling [[old-treatment-pattern]] in 3 notes
     2. [pdf-editor] reflect on 2 orphan debug notes
     3. [youtube]   restructure stale moc/personal-assistant-hybrid.md
   ```

5. **Save the cross-vault summary** to
   `A:/projects-vault/vault/process/health/<YYYY-MM-DD>-multi-vault-report.md`
   if `A:/projects-vault/` exists (it is the canonical meta-vault).
   Otherwise save to `.vault/ops/health/<YYYY-MM-DD>-multi-vault-report.md`
   of the current project. Always print the report to stdout regardless.

6. **Verify** no regressions:
   ```bash
   py scripts/vault/regression_guard.py
   ```

**START NOW.**

---

# /health Reference

## Mode matrix

| Invocation | Scope | Categories run | Speed |
|------------|-------|---------------|-------|
| `/health`          | this vault only (single) | Quick (1-3) | < 30 s |
| `/health full`     | this vault only          | Full (1-8)  | 1-3 min |
| `/health three-space` | this vault only       | Boundaries only (5) | < 30 s |
| `/health --all`    | every enabled vault     | Quick (1-3) per vault | depends on N |
| `/health full --all` | every enabled vault   | Full (1-8) per vault  | several min |

**Single-vault is always the default.** This is intentional:
- Existing skills and CI that call `/health` keep their behavior.
- `arscontexta:health` continues to work standalone with no awareness of
  the registry.
- Multi-vault is OPT-IN, not opt-out. Adopters must add `--all`.

## Why a project-local wrapper

The plugin skill `arscontexta:health` operates on a single working
directory — its design predates the cross-vault registry. Rather than
fork the plugin, this wrapper:

1. **Inherits everything** from `arscontexta:health` for the per-vault
   diagnostic — same 8 categories, same thresholds, same output format.
2. **Adds scope resolution** via `scripts/vault/multi_vault.py`, which is
   the single source of truth for vault enumeration.
3. **Adds aggregation** — a cross-vault summary that single-vault runs
   could never produce.
4. **Stays plugin-safe** — never modifies the plugin's cache; survives
   plugin updates intact.

## Per-vault report location

Each vault gets its own report under its own `ops/health/`:

```
A:/projects-vault/apps/pdf-editor/.vault/ops/health/2026-05-16-report.md
A:/projects-vault/apps/fitness-app/.vault/ops/health/2026-05-16-report.md
A:/projects-vault/apps/hair-app/.vault/ops/health/2026-05-16-report.md
A:/projects-vault/apps/youtube/.vault/ops/health/2026-05-16-report.md
```

The aggregated cross-vault summary lives at:

```
A:/projects-vault/vault/process/health/2026-05-16-multi-vault-report.md
```

This separation keeps each vault's history self-contained while giving
the meta-vault (`projects-vault`) a cross-cutting picture for trend
analysis.

## Cross-vault findings worth surfacing

Some health signals are only visible at the multi-vault level:

| Cross-vault finding | What it means |
|---------------------|---------------|
| Same dangling link target referenced in multiple vaults | Shared concept needs a canonical home — likely in `projects-vault` |
| Schema version mismatch across vaults | One vault was upgraded; others lagging — sync with central registry |
| Conflicting notes about the same concept | Trigger `methodology_conflict_resolution.md` protocol |
| One vault accumulates orphans while others stay healthy | Process throughput regression localized to that vault |
| All vaults orphan at similar rate | Methodology drift; consider `arscontexta:architect` |

## Failure modes & graceful degradation

| Symptom | Cause | Behavior |
|---------|-------|----------|
| Registry missing | `A:/projects-vault/vault/ops/registry.yaml` not found | Falls back to single-vault, warns once |
| One sibling vault on disconnected drive | path no longer exists | Skip that vault, continue with the rest |
| `arscontexta:health` plugin missing | plugin not installed | Run only the file-based checks (schema, orphans, links, frontmatter) — `multi_vault.py find` is enough for those |
| Obsidian has the vault open | `.obsidian/.lock` present | Skip write phase, still print to stdout |
| Disk full on report write | I/O error | Continue with next vault; flag in summary |

## See also

- `arscontexta:health` — single-vault diagnostic engine this skill wraps
- `scripts/vault/multi_vault.py` — vault enumeration adapter
- `A:/projects-vault/vault/ops/registry.yaml` — canonical vault list
- `.vault/process/methodology/methodology_multi_vault_skills.md` — pattern
  documentation


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
