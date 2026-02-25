# Technique Playbook (how to apply)

## Equivalence Partitioning (EP)
1. List partition dimensions: inputs, outputs, internal states, time dependencies.
2. Identify valid and invalid partitions.
3. Minimal set: at least one test per partition.
4. Add tests for risk + historical failures (often around boundaries and exceptions).

## Boundary Value Analysis (BVA)
1. Apply only to ordered partitions (ranges).
2. Identify boundaries (min/max, threshold edges).
3. Choose 2-point (boundary + adjacent) or 3-point (below/at/above).
4. Make expected results explicit (including error types/messages).

## Decision Tables
1. List conditions and actions.
2. Build feasible combinations (columns); mark impossible ones.
3. Coverage items = feasible columns; 100% = cover all feasible columns.
4. If too many columns, reduce by risk/impact with a clear justification.

## State Transitions
1. Model states and transitions (event/guard/action).
2. Choose criterion (all states / all valid transitions / all transitions).
3. Derive scenario sequences that traverse items with minimal overlap.
4. Add invalid transitions if masking is a risk.

## Statement/Branch/MC/DC
1. Decide measurement scope and exclusions.
2. Classify uncovered code: not reached / unreachable / policy-excluded.
3. Add tests with stronger assertions, not just execution.
4. Use MC/DC only when required; it can be costly.
