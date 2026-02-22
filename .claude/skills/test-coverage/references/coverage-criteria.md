# Coverage Criteria Quick Guide

## Black-box (spec/model driven)
- **Equivalence Partitioning (EP)**:
  - Coverage items: partitions (including invalid)
  - 100%: cover each partition at least once
- **Boundary Value Analysis (BVA)**:
  - Coverage items: boundary (and adjacent values)
  - 100%: cover all boundaries (and adjacents for 3-point)
- **Decision Table Testing**:
  - Coverage items: feasible columns (condition combinations)
  - 100%: cover all feasible columns (or justify reduction)
- **State Transition Testing**:
  - Coverage items: states and/or transitions
  - Common criteria: all states, all valid transitions, all transitions (valid+invalid)

## White-box (structure driven)
- **Statement coverage**: executed statements / executable statements
- **Branch coverage**: executed branches / total branches
  - Typically, 100% branch implies 100% statement, but not vice versa.
- **MC/DC (when safety/criticality demands)**:
  - Each condition shown to independently affect the decision outcome.

## “Pick one” principle (for formal exit criteria)
If you must define an exit gate, often **one criterion** is enough because criteria can include others.
But it is common to **differentiate targets by component** (risk differs by area).
