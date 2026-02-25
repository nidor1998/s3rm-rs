# Performance & Build Guide (Rust)

## Measure first
- Benchmark before/after (criterion or cargo bench)
- Profile hotspots (perf/flamegraph etc), then change one variable at a time

## Build/profile knobs (Cargo)
- [profile.release] opt-level / lto / codegen-units / debug, etc.
- Consider size vs speed trade-offs

## Common low-hanging fruit
- Avoid repeated allocations; reserve capacity when shape is known
- Prefer iterators carefully; watch for accidental clones
- Choose data structures intentionally (Vec vs VecDeque vs HashMap)
