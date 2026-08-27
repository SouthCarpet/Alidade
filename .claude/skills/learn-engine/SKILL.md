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
- If target is a Rust crate name: research its documentation
- If target is empty: ask what to research

**Execute these steps:**

1. **Fetch official documentation first** — do not rely on memory or training data
2. Search for examples, changelogs, and best practices
3. Identify what is relevant to Alidade (link-measurement engine/store/cli/app
   workspace; Windows-first, no telemetry beyond the configured measurement)
4. Extract key facts as atomic notes under `.vault/`
5. Identify which workspace crate(s) (`engine`, `store`, `cli`, `app`) would be affected
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

For measurement/throughput APIs (e.g. Cloudflare speed-test endpoints):
- Official API documentation (fetch directly)
- Rate limits, auth requirements (none expected — keep it that way)
- Response shape and accuracy caveats

For Rust crates:
- docs.rs / crates.io documentation (fetch directly)
- GitHub README
- Usage examples, MSRV, feature flags

## Extraction Focus

When researching for this app, focus on:

| Category | What to Capture |
|----------|-----------------|
| API Interface | Endpoint, parameters, response shape |
| Windows/ICMP specifics | Any interaction with the IP Helper API path (`cfg(windows)`) |
| Crate capabilities | Feature flags, MSRV, what it can/cannot do |
| Rate limits | Requests/min if the source imposes any |
| Integration pattern | How to call it from Rust (async/tokio, error type) |
| Failure modes | Common errors, timeouts, offline behavior |

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
