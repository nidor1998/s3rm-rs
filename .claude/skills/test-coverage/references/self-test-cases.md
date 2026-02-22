# Self-Test Prompts (expected elements)

## A) Unit: numeric boundaries
Input: “validateAge(age): valid 0–120. Improve coverage.”
Expect:
- EP + BVA partitions/boundaries
- Minimal set → prioritized additions
- Expected results explicit
- Limitations noted

## B) Integration: combinations
Input: “Discount rules depend on isMember/hasCoupon/isFirstPurchase/cartTotal>=5000.”
Expect:
- Decision table (feasible columns)
- Reduction strategy if too many
- Table-form test cases

## C) E2E: state transitions
Input: “Checkout flow: cart → payment → inventory → shipping.”
Expect:
- State/transition model
- Chosen criterion (all states/transitions)
- Observables (UI/API/DB/events)

## D) White-box: branches
Input: “Branch coverage is low in a module. What next?”
Expect:
- Uncovered classification (not reached/unreachable/excluded)
- Branch vs statement relation explained
- Next tests emphasize assertions

## E) Mutation
Input: “Coverage is high but bugs escaped. Try mutation testing.”
Expect:
- Explains surviving mutants meaning
- Scope limitation + cost caveats
- Concrete hardening actions

## F) Risk prioritization
Input: “No time. Focus tests on highest risk.”
Expect:
- Likelihood × impact framing
- Prioritized scope
- Component-level targets
