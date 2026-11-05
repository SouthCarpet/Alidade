---
name: choice-page
description: Use for every decision for Michal that has more than one question or needs visual proposals (previews, screenshots, live demos). Builds a local HTML choice page, serves it, and gets the answer back to the session without a download. One simple question may stay in chat.
---

# Choice page

Michal decides on a local HTML page, not in a yes/no list in chat. The page
runs on `127.0.0.1`, the answer returns to the session by itself.
Tool: `py A:/projects-vault/scripts/choice_page.py`. Spec and answer schema:
`scripts/README.md`, section "choice_page.py".

## Steps

1. **Write the spec.** One JSON file, for example
   `animus/data/agent-runs/<plan>/choice-spec.json`. Put media (images, gif,
   video, a live demo folder) under `assets_root`. Prefer a live preview
   (`iframe`) or numbers for visual choices. Every choice card (`single`,
   `multi`, `table`) needs `what`: what the thing does, who uses it and what
   state it is in, so Michal decides without searching. The build fails
   without it. Every card also needs `why` and may add at most two `blocks`
   (`{label, text}`). Give a `recommended` option only with a
   `recommend_reason`. Write the text in plain Slovak (ASD-STE100 style).
   A card about a Senate or a Council carries `origin` (`kind`, `stage`, and
   `id`, `reason`, `report` when known), and its options carry `seat`
   (`provider / model`). A `convene` choice on such a card is Michal's written
   approval: the `USER DECISION` note cites the answer file, and the session
   passes that citation in `--reason` to `council.py convene --approved-by-user`
   or when it starts a Senate. Adoption of a Council verdict still goes
   through `council.py adopt`, by Michal.
2. **Serve it in the background.**
   `py A:/projects-vault/scripts/choice_page.py serve <spec> --out-dir animus/data/agent-runs/<plan> --port 0`
   - T3: run it with the Bash tool in the background, wait with
     `t3_background_wait`. When `URL: ...` appears, open it with
     `mcp__t3-code__preview_open`, so the page shows in the thread.
   - Plain CLI: add `--open` (opens the default browser).
   - Port 4190 is the default. `--port 0` picks a free one.
   - The server exits by itself after one answer (`ANSWER: <path>`, exit 0)
     or after `--timeout-min` (`TIMEOUT`, exit 3). To wait for a file from
     another run: `choice_page.py wait <dir>`.
3. **Read the answer.** When `ANSWER:` appears:
   `py A:/projects-vault/scripts/choice_page.py read <answer.json> --spec <spec> [--json]`.
   Tell Michal only the differences from the recommendation and the notes.
   Copy nothing: the file already is in `animus/data/agent-runs/<plan>/`.
   Then log it:
   `py A:/projects-vault/animus/core/animus.py event append --json '<json>'`
   with `{"plan_id": "<plan>", "phase_id": "<phase>", "task_id": "<task>", "event_type": "task_event", "notes": "USER DECISION <file> ..."}`.
   The `notes` start with `USER DECISION <answer file name> ...`. When a
   choice approves a rule, add `APPROVED: <rule file>` (otherwise
   `lint_rules.py` refuses the row; example: ledger seq 14921). Put the table
   of choices into the plan record.
4. **Lessons.**
   - Read a choice no wider than it is. A pick of B over A for one place is
     not a ban on A. When two styles are compared, use the `where`
     sub-question or a `table` question ("which variant where").
   - Show numbers and previews for measured results.
   - Never invent a recommendation without a reason.
   - A `html` media block is trusted: only the controller writes it.
