# Mutation Testing

## What it is
- Introduce small intentional code changes (“mutants”)
- Run tests: if tests fail, the mutant is “killed”
- Surviving mutants suggest tests/ assertions may be insufficient

## When to use
- Coverage is high but defects still escape
- You suspect weak assertions or insufficient behavioral checks
- You want to harden critical logic regression

## Caveats
- Expensive to run; start with critical modules or changed code
- Equivalent mutants can create noise; triage is required
