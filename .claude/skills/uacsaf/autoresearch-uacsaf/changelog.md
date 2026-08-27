# Autoresearch Changelog: uacsaf

## Eval Criteria
- EVAL 1: Mode classified — Does the output explicitly classify a debug mode (BUG_HUNT, SECURITY_REVIEW, PERFORMANCE_INCIDENT, etc.)?
- EVAL 2: Structured findings — Does the output include at least 1 numbered issue with CONFIDENCE, CLASSIFICATION, SEVERITY meta-fields?
- EVAL 3: Evidence-based — Does every finding cite specific code locations (file + function/line)?
- EVAL 4: Fix proposed — Does the output include a MINIMAL_FIX or PATCH_PLAN for at least 1 issue?

## Test Inputs
1. "Debug why Flask app.py route handling could fail with 500 errors"
2. "Security review of monetization_compliance.py for input validation and data exposure"
3. "Root-cause analysis of channel_memory.py data persistence behavior"
4. "Performance analysis of gemini_trends.py API call patterns and aggregation"

---

## Experiment 0 — baseline

**Score:** pending
**Change:** None
