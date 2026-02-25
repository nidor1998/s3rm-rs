# Code Style & Conventions

## Rust Conventions
- Rust 2024 edition
- All code must pass `cargo clippy` with zero warnings
- Code must be formatted with `cargo fmt`
- Public APIs documented with rustdoc comments
- Async throughout - all I/O uses async/await with Tokio

## Naming
- Rust standard: snake_case for functions/variables, PascalCase for types/traits
- Test files: `*_properties.rs` for property-based tests, `*_unit_tests.rs` for unit tests
- Module files follow Rust conventions (mod.rs for directory modules)

## Testing
- Property-based tests use proptest crate
- Tests co-located with source code
- Unit tests in `#[cfg(test)]` modules
- Tests must never freeze or wait for user input
- Use `#[tokio::test]` for async tests

## Symbol Name Paths in Serena
- For Rust impl methods, use: `impl StructName/method_name`
- NOT `StructName/method_name` (this won't find methods)
- Traits: `Deleter` (kind: Interface)
- Trait impls: `impl Deleter for BatchDeleter`
