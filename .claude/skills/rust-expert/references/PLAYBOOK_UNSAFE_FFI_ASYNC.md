# Unsafe / FFI / Async Playbook

## Unsafe guidance
- Minimize unsafe surface area
- Wrap unsafe in safe API
- State invariants explicitly:
    - pointer validity + alignment
    - aliasing rules
    - initialization guarantees
    - lifetime requirements
- Prefer standard patterns over ad-hoc transmute

## FFI guidance (Rust <-> C)
- Use `extern "C"` for C ABI
- Use `#[repr(C)]` for structs crossing the boundary
- Avoid panics crossing FFI; return error codes
- Ownership rules at boundary:
    - who allocates / who frees
    - stable layout + null handling for pointers
- Consider unwind behavior carefully

## Async/await guidance
- Identify runtime (Tokio/async-std/etc) and constraints
- Avoid blocking in async; use spawn_blocking or dedicated threads
- Think about cancellation, timeouts, backpressure
- For async trait patterns, consider object safety and lifetimes
