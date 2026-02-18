# Rust Testing Patterns for s3rm-rs

## Property-Based Testing with Proptest

### Basic Structure
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn property_name(input in strategy()) {
        // Test universal property
        prop_assert!(condition);
    }
}
```

### Requirement Annotation
Always annotate property tests:
```rust
// **Property 1: Batch Deletion API Usage**
// **Validates: Requirements 1.1, 5.5**
proptest! {
    #[test]
    fn batch_deletion_uses_correct_api(objects in vec(any::<S3Object>(), 1..1000)) {
        // Test implementation
    }
}
```

## Unit Testing Patterns

### Test Organization
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specific_behavior() {
        // Arrange
        let input = setup();
        
        // Act
        let result = function(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

## Performance Guidelines

- Keep unit tests under 60 seconds total
- Use small input sizes for property tests
- Limit proptest cases: `#![proptest_config(ProptestConfig::with_cases(100))]`
- Mock expensive operations when appropriate
