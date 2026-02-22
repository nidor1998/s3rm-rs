# Evaluation Rubric (0–3 each, max 21)

1) Problem framing & assumptions
- 0: assumes without stating
- 1: lists assumptions but doesn’t validate
- 2: flags key unknowns
- 3: clarifies boundary/goal/constraints/current state succinctly

2) Coverage criteria selection
- 0: lists metrics only
- 1: generic criteria, weak linkage to goal
- 2: selects criteria with reasons
- 3: includes alternatives, inclusion relations, component-level differentiation (risk)

3) Coverage item extraction
- 0: none
- 1: vague
- 2: concrete and technique-aligned
- 3: traceable mapping: criterion → items → tests

4) Test design quality
- 0: ad hoc
- 1: only typical cases
- 2: includes negative/edge/state aspects
- 3: minimal set + prioritized additions; observables/expected results specified

5) Measurement & interpretation
- 0: shows only %
- 1: no meaning for uncovered parts
- 2: classifies uncovered areas (not reached/unreachable/excluded)
- 3: proposes concrete next actions and guardrails

6) Risk & limitations
- 0: absent
- 1: generic
- 2: explains “high coverage ≠ no defects” with examples
- 3: prevents misuse (gaming, over-testing, false confidence)
