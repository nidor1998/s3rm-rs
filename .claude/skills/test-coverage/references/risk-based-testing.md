# Risk-Based Testing (practical template)

## Risk model
- Risk attributes: **likelihood** × **impact**
- Risk types: project risk (schedule/resources) vs product risk (quality attributes, failure impact)

## Flow
1. List product risks: critical flows, failure history, complex/new areas, external integrations, compliance.
2. Score likelihood and impact (quantitative or qualitative).
3. Translate into test scope + priorities:
   - High risk: stronger criteria, more negative/edge cases, stronger regression automation
   - Low risk: minimal set + smoke
4. Differentiate coverage targets by component when justified.

## Prioritization styles (choose deliberately)
- Risk-based: run highest risk first
- Coverage-based: achieve broad coverage early
- Requirements-based: cover highest-priority requirements first
