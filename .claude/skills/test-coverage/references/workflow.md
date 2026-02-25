# Test Coverage Workflow (practical)

## 1) Pin down the goal
- Compliance/exit gate? (e.g., “branch ≥ 80%”)
- Defect discovery? (likely missing partitions/edges/states)
- Faster regression? (prioritize critical paths + stable automation)
- Avoid catastrophic failures? (product risk first)

## 2) Decompose the SUT
- Boundary: inputs/outputs/state/side effects (DB, messages, external APIs)
- Spec structure: ranges, options, rules, state transitions, exceptions
- Implementation structure: branches, exception paths, unreachable code, complex predicates, critical modules

## 3) Choose coverage criteria (start with a minimal set)
- Specification/model-based: EP/BVA/decision tables/state transitions
- Structure-based: statement/branch (MC/DC if warranted)
- Experience-based: error guessing/exploratory charters

Rule of thumb: map **goal → criteria → expected defect types**.

## 4) Extract coverage items
- EP: partitions (valid + invalid)
- BVA: boundary and adjacent values (2-point or 3-point)
- Decision table: feasible columns (condition combinations)
- State transitions: states/transitions (choose a transition criterion)
- White-box: executable statements/branches/paths of interest

## 5) Turn items into tests
- Build the **minimal 100% set** for the chosen criterion
- Add prioritized tests for risks, past failures, changes, and critical assertions
- Ensure expected results are **observable** (return values, logs, DB state, events)

## 6) Measure → interpret → improve
- Don’t report only %: explain what’s not covered and why
- Classify uncovered code: not reached / unreachable / excluded by policy
- If coverage is high but bugs appear: suspect weak assertions, missing combinations/states, or environment gaps
