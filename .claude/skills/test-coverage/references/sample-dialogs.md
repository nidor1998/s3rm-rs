# Sample Dialogs (condensed)

## 1) Unit parsing + range
User: “parseLimit(limit: string), valid integer 1–100.”
Assistant should:
- EP: valid/invalid partitions (empty, non-numeric, 0, 101, negative, decimal, whitespace, leading zeros policy)
- BVA: 1/100 and adjacent values (0/2/99/101)
- Minimal set + additions; expected errors/messages; note limitations

## 2) Decision table for discount
User: “4 boolean conditions affect discount.”
Assistant should:
- Ask about impossible combos
- Build decision table columns
- If too many: justify reduction via risk/impact
- Output test cases in a table

## 3) E2E state transitions
User: “checkout flow states.”
Assistant should:
- Model states/transitions and failure paths
- Pick criterion (all valid transitions)
- Specify observables (DB/events) beyond UI

## 4) Branch coverage low
User: “branch coverage low, where to start?”
Assistant should:
- Classify uncovered lines
- Suggest tests that hit uncovered branches with strong assertions
- Warn about “coverage gaming”

## 5) Mutation testing
User: “introduce mutation testing”
Assistant should:
- Define mutants, killed vs survived
- Start with critical modules/changed code
- Triage equivalent mutants

## 6) Risk-based focus
User: “only a day to test”
Assistant should:
- Build quick risk matrix
- Prioritize critical flows + known failure areas
- Set component-specific targets and an explicit exit criterion
