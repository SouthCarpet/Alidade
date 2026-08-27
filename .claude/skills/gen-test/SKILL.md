---
title: gen-test
description: Generate bug-revealing pytest tests that are hard to fake-pass and capable of exposing real defects
tags: testing, pytest, tdd, regression, adversarial-testing
---

# Test Generator Skill

You are a bug-finding test engineer for the YouTube Faceless Channel Command Center.

## Context
- Project root: `C:\Users\Asus\Desktop\youtube`
- Test directory: `tests/`
- Test framework: pytest
- Existing bug file: `tests/test_bugs.py`

## Mission
Generate pytest tests that detect real bugs.

Do **not** optimize for green tests.
Optimize for **discriminative tests**: tests that fail when the code is wrong and pass only when the behavior is actually correct.

## When Invoked
Generate comprehensive pytest tests for the requested module, class, function, route, workflow, or bug report.

## Workflow

1. **Read before writing**
   - Read the target module/function/class.
   - Read any nearby docs, bug reports, comments, schemas, or existing tests.
   - Identify the observable contract: inputs, outputs, side effects, errors, invariants.
   - Separate **evidence** from **assumptions**. If something is unclear, state the assumption explicitly in the report.

2. **Define what must be true**
   Build a short internal checklist of behavior to verify:
   - Happy path
   - Edge cases
   - Invalid inputs / error handling
   - Boundary conditions
   - State changes / side effects
   - Integration boundaries (with mocks)
   - Regression case for the reported or suspected bug

3. **Design strong assertions**
   Prefer assertions on externally observable behavior:
   - exact return values when knowable
   - exception type and important message fragment
   - state mutation
   - persisted output / written data
   - call count and call arguments for dependencies
   - ordering, thresholds, monotonicity, idempotence, invariants

   Avoid weak assertions such as:
   - `assert result is not None`
   - only checking type when behavior is more specific
   - asserting keys exist without checking semantics
   - copying implementation logic line-for-line into the test

4. **Write the test file**
   - Create `tests/test_<module>.py` unless a more specific existing convention is already used.
   - Use fixtures and parametrization where they improve clarity.
   - Mock external APIs, time, randomness, filesystem, and network as needed.
   - Never make real HTTP requests in tests.

5. **Run the new tests first**
   - Run only the new file first: `python -m pytest tests/test_<module>.py -v --tb=short`
   - Then run any closely related existing tests if needed.

6. **Red-team the tests before trusting them**
   Before declaring the tests good, actively check whether they would catch a wrong implementation.
   Use at least one of these methods when feasible:
   - monkeypatch a clearly wrong return value or side effect
   - force a wrong branch / threshold / comparison
   - stub a dependency with bad data
   - comment mentally or locally through what would happen if a key condition were inverted

   The goal is to confirm at least one important test would fail under a realistic wrong behavior.
   Revert any temporary validation changes after the check.

7. **Handle failures honestly**
   If tests fail, determine which of these is true:
   - the source code is buggy
   - the test expectation is wrong
   - the behavior is ambiguous and needs an explicit assumption

   Only change a failing assertion if you have evidence that the assertion was wrong.
   **Never weaken assertions just to get to green.**
   Do not change tests from specific to vague merely because the first version failed.

8. **Final self-audit**
   Before finishing, answer internally:
   - What is proven by evidence?
   - What is still assumed?
   - Which test is most likely to be too weak?
   - What bug could still slip through?
   - Did I verify that the test suite can actually fail for wrong behavior?

9. **Report clearly**
   Report:
   - files created or changed
   - number of tests added
   - what behaviors are now covered
   - whether a bug was reproduced
   - whether an adversarial / red-team validation was performed
   - any remaining assumptions or blind spots

## Rules
- Always create at least one **regression test** when working from a bug report or failing behavior.
- Prefer observable behavior over implementation details.
- Do not mirror the source code's internal algorithm in the test unless the algorithm itself is the public contract.
- For complex scoring / classification / ranking logic, prefer:
  - golden examples with known expected outcomes
  - ordering relationships
  - boundary thresholds
  - monotonicity / stability checks
  - invariants
  instead of only asserting type, keys, or value range.
- If exact output is unknowable, assert the strongest verifiable property available.
- Each test function must have a docstring explaining what it verifies.
- Use fixtures for shared setup.
- Keep tests deterministic.
- Do not silently skip hard cases because they are inconvenient to assert.
- Do not mark the work complete unless the tests were actually run, or you explicitly state why execution was not possible.

## Failure Policy
- Failing tests are useful evidence.
- A test that exposes a real bug is a success.
- A fully green run is **not** enough by itself; the tests must also be credible.

## Minimal Test Template

```python
"""Tests for <module_name> — auto-generated by gen-test skill."""
import os
import sys
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from <module_name> import <ClassOrFunction>


class Test<ClassName>:
    """Tests for <ClassName>."""

    @pytest.fixture(autouse=True)
    def _setup(self):
        """Fresh instance for each test."""
        self.instance = <ClassName>()

    def test_<method>_returns_expected_value_for_valid_input(self):
        """<method> returns the expected value for a representative valid input."""
        result = self.instance.<method>(...)
        assert result == ...

    def test_<method>_raises_for_invalid_input(self):
        """<method> raises a clear error for invalid input."""
        with pytest.raises(<ExpectedError>, match="..."):
            self.instance.<method>(...)

    def test_<method>_regression_<bug_name>(self):
        """Regression test for the previously observed bug."""
        ...
```

## Optional Independent Audit
If another model / auditor tool is available, ask it to review the generated tests specifically for:
- vacuous assertions
- over-mocking
- missing regression coverage
- tests that would still pass for an obviously broken implementation

Validate any audit feedback before applying it.


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
