---
name: test-coverage
description: Improve test coverage (design + measurement). Use when you need coverage criteria, test case design, coverage measurement, missing tests, regression optimization, risk-based prioritization, boundary values, state transitions, branches/MC/DC, or mutation testing.
argument-hint: "[scope] [goal: quality/speed/compliance/regression] [constraints]"
allowed-tools: Read, Grep, Glob
---

# Test Coverage Expert (for Claude Code)

## What this skill does
Help you **increase meaningful test coverage** (not just a percentage) by:
- choosing coverage criteria that match the goal,
- deriving **coverage items** (what must be covered),
- designing minimal test sets + prioritized additions,
- interpreting coverage reports and turning them into next actions,
- using risk to focus effort.

## Always confirm first (ask if missing)
- **SUT boundary:** module/API/UI/job, inputs/outputs, state, side effects (DB, queues, external APIs)
- **Goal:** defect discovery, faster regression, compliance/exit criteria, change-risk minimization
- **Constraints:** time, people, environments, CI limits, release cadence, risk tolerance, regulated domain?
- **Current state:** test levels (unit/integration/E2E), flaky tests, existing coverage tooling & reports

## Output format (use this order every time)
1) **Goal & assumptions** (call out unknowns)
2) **Recommended coverage criteria** (ranked + why)
3) **Coverage items** (table or bullets; traceable to criteria)
4) **Test design** (minimal set → prioritized additions; include data + expected results/observables)
5) **Measurement & reporting** (what to measure, exclusions, thresholds, how to interpret)
6) **Risks & limitations** (e.g., “high coverage ≠ no defects”; what coverage won’t show)

## Safety & constraints
- Default is **read-only**: no Write/Bash unless you explicitly allow it.
- Do not rely on fetching external URLs by default.
- Never include secrets (tokens/keys/PII) in outputs.

### MCP
- context7
- serena

## References (open only if needed)
- Workflow: references/workflow.md
- Coverage criteria & inclusion relations: references/coverage-criteria.md
- Technique playbook: references/techniques.md
- Measurement & reporting: references/measurement-and-reporting.md
- Risk-based testing: references/risk-based-testing.md
- Mutation testing: references/mutation-testing.md
- Evaluation rubric: references/evaluation-rubric.md
- Self-test prompts: references/self-test-cases.md
- Sample dialogs: references/sample-dialogs.md
