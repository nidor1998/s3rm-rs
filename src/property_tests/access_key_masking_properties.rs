// Property-based tests for access key masking.
//
// Feature: s3rm-rs, Property: Access Key Masking
// For any access key string, the masking function must preserve the output
// length, show only the first 4 and last 4 characters for keys of 9+ chars,
// fully mask shorter keys, and never leak the original middle portion.
// **Validates: Requirements 4.10 (credential security in logs)**

#[cfg(test)]
mod tests {
    use crate::types::AccessKeys;
    use crate::types::mask_access_key;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Generators
    // -----------------------------------------------------------------------

    /// Generate a random access key string with ASCII alphanumeric characters.
    fn arb_access_key() -> impl Strategy<Value = String> {
        "[A-Za-z0-9/+]{0,64}"
    }

    /// Generate a realistic AWS access key ID (20 chars, starts with AKIA).
    fn arb_aws_access_key_id() -> impl Strategy<Value = String> {
        "[A-Z0-9]{16}".prop_map(|suffix| format!("AKIA{suffix}"))
    }

    // -----------------------------------------------------------------------
    // Properties
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// Masked output must always have the same length as the input.
        #[test]
        fn prop_mask_preserves_length(key in arb_access_key()) {
            let masked = mask_access_key(&key);
            prop_assert_eq!(
                masked.len(),
                key.len(),
                "Masked length must equal input length for key of {} chars",
                key.len()
            );
        }

        /// Keys shorter than 9 characters must be fully masked with asterisks.
        #[test]
        fn prop_short_keys_fully_masked(key in "[A-Za-z0-9]{0,8}") {
            let masked = mask_access_key(&key);
            prop_assert!(
                masked.chars().all(|c| c == '*'),
                "Short key ({} chars) must be fully masked, got: {}",
                key.len(),
                masked
            );
        }

        /// Keys of 9+ characters must show exactly the first 4 and last 4
        /// characters from the original, with asterisks in between.
        #[test]
        fn prop_long_keys_show_prefix_and_suffix(key in "[A-Za-z0-9]{9,64}") {
            let masked = mask_access_key(&key);

            // First 4 characters preserved
            prop_assert_eq!(
                &masked[..4],
                &key[..4],
                "First 4 chars must be preserved"
            );
            // Last 4 characters preserved
            prop_assert_eq!(
                &masked[masked.len() - 4..],
                &key[key.len() - 4..],
                "Last 4 chars must be preserved"
            );
        }

        /// The middle portion of long keys must be all asterisks.
        #[test]
        fn prop_long_keys_middle_all_asterisks(key in "[A-Za-z0-9]{9,64}") {
            let masked = mask_access_key(&key);
            let middle = &masked[4..masked.len() - 4];
            prop_assert!(
                middle.chars().all(|c| c == '*'),
                "Middle portion must be all asterisks, got: {}",
                middle
            );
        }

        /// The middle portion of the original key must never appear in the
        /// masked output (for keys long enough to have a non-trivial middle).
        #[test]
        fn prop_original_middle_not_leaked(key in "[A-Za-z0-9]{10,64}") {
            let masked = mask_access_key(&key);
            let original_middle = &key[4..key.len() - 4];
            // Only check if middle is non-empty and not all the same char
            // (e.g., "AAAA" middle could appear in prefix/suffix by coincidence)
            if original_middle.len() > 1 {
                prop_assert!(
                    !masked.contains(original_middle),
                    "Masked output must not contain the original middle: {}",
                    original_middle
                );
            }
        }

        /// Realistic AWS access key IDs (AKIA..., 20 chars) must be properly
        /// masked with first 4 and last 4 visible.
        #[test]
        fn prop_aws_key_id_masked_correctly(key in arb_aws_access_key_id()) {
            let masked = mask_access_key(&key);
            prop_assert_eq!(masked.len(), 20);
            prop_assert!(masked.starts_with("AKIA"));
            prop_assert_eq!(&masked[masked.len() - 4..], &key[key.len() - 4..]);

            let middle = &masked[4..16];
            prop_assert!(
                middle.chars().all(|c| c == '*'),
                "Middle 12 chars must be asterisks for AWS key, got: {}",
                middle
            );
        }

        /// The Debug impl of AccessKeys must never contain the full access key.
        #[test]
        fn prop_debug_never_leaks_full_access_key(key in "[A-Za-z0-9]{9,40}") {
            let access_keys = AccessKeys {
                access_key: key.clone(),
                secret_access_key: "secret".to_string(),
                session_token: None,
            };
            let debug_string = format!("{access_keys:?}");
            prop_assert!(
                !debug_string.contains(&key),
                "Debug output must not contain the full access key"
            );
        }

        /// The Debug impl of AccessKeys must never contain the secret access key.
        #[test]
        fn prop_debug_never_leaks_secret_key(
            key in "[A-Za-z0-9]{9,40}",
            secret in "[A-Za-z0-9/+]{20,60}",
        ) {
            let access_keys = AccessKeys {
                access_key: key,
                secret_access_key: secret.clone(),
                session_token: None,
            };
            let debug_string = format!("{access_keys:?}");
            prop_assert!(
                !debug_string.contains(&secret),
                "Debug output must not contain the secret access key"
            );
        }

        /// The Debug impl of AccessKeys must never contain the session token.
        #[test]
        fn prop_debug_never_leaks_session_token(
            key in "[A-Za-z0-9]{9,40}",
            token in "[A-Za-z0-9/+]{20,60}",
        ) {
            let access_keys = AccessKeys {
                access_key: key,
                secret_access_key: "secret".to_string(),
                session_token: Some(token.clone()),
            };
            let debug_string = format!("{access_keys:?}");
            prop_assert!(
                !debug_string.contains(&token),
                "Debug output must not contain the session token"
            );
        }
    }
}
