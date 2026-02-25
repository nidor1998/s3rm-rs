# Measurement & Reporting

## Decide up front
- **What is measured:** product code vs tests vs generated/vendor code
- **Granularity:** file/module/function/line/branch
- **Exclusions:** unreachable code, platform-specific paths, non-critical modules (must be justified)
- **Thresholds:** if used as a quality gate, specify by component and rationale
- **Change tracking:** how to treat PR deltas (prevent regressions vs allow temporary dips)

## Interpretation rules
- % is a summary; the meaning depends on **what is uncovered**.
- 100% coverage does not guarantee correctness:
  - weak assertions,
  - missing combinations,
  - missing state/temporal behaviors,
  - environment-specific failures.

## Recommended short report format
- Goal (why measure)
- Measurement setup (scope/exclusions/thresholds)
- Current state (key numbers + what’s uncovered)
- Next actions (top 3)
- Limits (what coverage does not show)
