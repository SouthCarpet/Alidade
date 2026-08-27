---
name: learn-engine
description: Research a new technology, pattern, or architectural concept and capture findings as engine specs. Use when adding a new engine, evaluating a new AI model, or understanding an external API. Triggers on "/learn", "/learn [topic]", "research this", "learn about", "investigate".
user-invocable: true
allowed-tools: Read, Write, Glob, WebSearch, WebFetch
context: fork
---

## EXECUTE NOW

**Target: $ARGUMENTS**

- If target is a technology/API name: research it and capture findings
- If target is a Python module name: research its documentation
- If target is empty: ask what to research

**Execute these steps:**

1. **Fetch official documentation first** — do not rely on memory or training data
2. Search for examples, changelogs, and best practices
3. Identify what is relevant to this YouTube automation pipeline
4. Extract key facts as atomic engine spec notes
5. Identify which existing engines would be affected
6. Add to relevant system maps

**START NOW.**

---

# Learn-Engine

Research external knowledge and integrate it into the vault.

## Documentation Fetching (REQUIRED)

**Always fetch the official docs before writing any findings.** Use `defuddle parse <url> --md` (preferred) or WebFetch as fallback.

For APIs, always fetch:
- Main API docs overview page
- Authentication page (if separate) — auth field names from official docs are ground truth
- Any sub-pages for the specific endpoints being used (e.g. search, download)

> Rationale: Auth field names differ per provider (e.g. Freesound uses "Client secret/Api key" as the token, not a separately-named API key). Training data is often wrong or outdated. Official docs are the only reliable source.

## Research Sources

For AI APIs and models:
- Official API documentation (fetch directly)
- Model capability comparisons
- Rate limits and pricing
- Integration patterns with Python

For Python libraries:
- PyPI documentation (fetch directly)
- GitHub README
- Usage examples

## Extraction Focus

When researching for this pipeline, focus on:

| Category | What to Capture |
|----------|-----------------|
| Auth fields | Exact field names as shown in official docs |
| API Interface | Endpoint, parameters, response shape |
| Model capabilities | What it can/cannot do |
| Rate limits | Requests/min, token limits |
| Cost | Per-token or per-request pricing |
| Integration pattern | How to call it from Python |
| Failure modes | Common errors, timeouts, quotas |

## Output

```
## Learn Complete: [topic]

### Findings
- [[finding spec]] — [key fact]
- [[finding spec]] — [key fact]

### Impact on Existing Engines
- [[engine spec]] needs updating because [reason]

### New Capabilities Identified
- [what this enables in the pipeline]
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
