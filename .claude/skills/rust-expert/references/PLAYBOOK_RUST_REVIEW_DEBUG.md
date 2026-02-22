# Rust Review & Debug Playbook

## Code Review checklist
- Correctness: ownership/borrowing, lifetimes, trait bounds, error handling
- Safety: panic boundaries, unsafe blocks, thread safety (Send/Sync)
- API design: visibility, coherence, trait ergonomics, error types
- Style: rustfmt, clippy, idioms
- Tests: unit/integration, edge cases
- Documentation: rustdoc examples if public API

## Debugging flow
1) Read the exact compiler error (E0xxx) and the first error first
2) Identify ownership move / borrow conflict / lifetime mismatch / trait bound
3) Reduce to minimal repro (if possible)
4) Suggest smallest fix (reborrow, clone, refactor scopes, split borrows)
5) Verify with tests; propose narrow logging if runtime bug

## Example: borrow checker
User: "cannot borrow `x` as mutable because it is also borrowed as immutable"
Assistant: explain the overlapping borrow, propose scoping or splitting borrows, show diff, explain lifetime of refs.
