# E2E Test Cases -- s3rm-rs

> Generated from `steering/init_build/e2e_test_plan.md`

## Test Case Index

| Test ID | Name | File | Requirements |
|---------|------|------|-------------|
| 29.0 | [Infrastructure](#test-290-infrastructure) | `tests/common/mod.rs` | - |
| 29.1 | [Include Regex Filter](#test-291-include-regex-filter) | `tests/e2e_filter.rs` | 2.2 |
| 29.2 | [Exclude Regex Filter](#test-292-exclude-regex-filter) | `tests/e2e_filter.rs` | 2.2 |
| 29.3 | [Include Content Type Regex Filter](#test-293-include-content-type-regex-filter) | `tests/e2e_filter.rs` | 2.3 |
| 29.4 | [Exclude Content Type Regex Filter](#test-294-exclude-content-type-regex-filter) | `tests/e2e_filter.rs` | 2.3 |
| 29.5 | [Include Metadata Regex Filter](#test-295-include-metadata-regex-filter) | `tests/e2e_filter.rs` | 2.4 |
| 29.6 | [Exclude Metadata Regex Filter](#test-296-exclude-metadata-regex-filter) | `tests/e2e_filter.rs` | 2.4 |
| 29.6a | [Include Metadata Regex Multiple Entries](#test-296a-include-metadata-regex-multiple-entries) | `tests/e2e_filter.rs` | 2.4 |
| 29.6b | [Exclude Metadata Regex Multiple Entries Alternation](#test-296b-exclude-metadata-regex-multiple-entries-alternation) | `tests/e2e_filter.rs` | 2.4 |
| 29.7 | [Include Tag Regex Filter](#test-297-include-tag-regex-filter) | `tests/e2e_filter.rs` | 2.5 |
| 29.8 | [Exclude Tag Regex Filter](#test-298-exclude-tag-regex-filter) | `tests/e2e_filter.rs` | 2.5 |
| 29.8a | [Include Tag Regex Multiple Tags](#test-298a-include-tag-regex-multiple-tags) | `tests/e2e_filter.rs` | 2.5 |
| 29.8b | [Exclude Tag Regex Multiple Tags Alternation](#test-298b-exclude-tag-regex-multiple-tags-alternation) | `tests/e2e_filter.rs` | 2.5 |
| 29.9 | [Mtime Before Filter](#test-299-mtime-before-filter) | `tests/e2e_filter.rs` | 2.7 |
| 29.10 | [Mtime After Filter](#test-2910-mtime-after-filter) | `tests/e2e_filter.rs` | 2.7 |
| 29.11 | [Smaller Size Filter](#test-2911-smaller-size-filter) | `tests/e2e_filter.rs` | 2.6 |
| 29.12 | [Larger Size Filter](#test-2912-larger-size-filter) | `tests/e2e_filter.rs` | 2.6 |
| 29.13 | [Basic Prefix Deletion](#test-2913-basic-prefix-deletion) | `tests/e2e_deletion.rs` | 1.1, 2.1 |
| 29.14 | [Batch Deletion Mode](#test-2914-batch-deletion-mode) | `tests/e2e_deletion.rs` | 1.1, 1.9 |
| 29.15 | [Single Deletion Mode](#test-2915-single-deletion-mode) | `tests/e2e_deletion.rs` | 1.2 |
| 29.16 | [Delete Entire Bucket Contents](#test-2916-delete-entire-bucket-contents) | `tests/e2e_deletion.rs` | 2.10 |
| 29.17 | [Empty Bucket No Error](#test-2917-empty-bucket-no-error) | `tests/e2e_deletion.rs` | 1.8 |
| 29.18 | [Dry Run No Deletion](#test-2918-dry-run-no-deletion) | `tests/e2e_safety.rs` | 3.1 |
| 29.19 | [Max Delete Threshold](#test-2919-max-delete-threshold) | `tests/e2e_safety.rs` | 3.6 |
| 29.20 | [Force Flag Skips Confirmation](#test-2920-force-flag-skips-confirmation) | `tests/e2e_safety.rs` | 3.4, 13.2 |
| 29.21 | [Versioned Bucket Creates Delete Markers](#test-2921-versioned-bucket-creates-delete-markers) | `tests/e2e_versioning.rs` | 5.1 |
| 29.22 | [Delete All Versions](#test-2922-delete-all-versions) | `tests/e2e_versioning.rs` | 5.2, 5.4 |
| 29.23 | [Delete All Versions Unversioned Bucket Error](#test-2923-delete-all-versions-unversioned-bucket-error) | `tests/e2e_versioning.rs` | 5.2 |
| 29.24 | [Rust Filter Callback](#test-2924-rust-filter-callback) | `tests/e2e_callback.rs` | 2.9, 12.5 |
| 29.25 | [Rust Event Callback](#test-2925-rust-event-callback) | `tests/e2e_callback.rs` | 7.6, 7.7, 12.6 |
| 29.26 | [Lua Filter Callback](#test-2926-lua-filter-callback) | `tests/e2e_callback.rs` | 2.8, 2.12 |
| 29.27 | [Lua Event Callback](#test-2927-lua-event-callback) | `tests/e2e_callback.rs` | 2.12, 7.6 |
| 29.28 | [Lua Sandbox Blocks OS Access](#test-2928-lua-sandbox-blocks-os-access) | `tests/e2e_callback.rs` | 2.13 |
| 29.29 | [Lua VM Memory Limit](#test-2929-lua-vm-memory-limit) | `tests/e2e_callback.rs` | 2.14 |
| 29.30 | [Rust Filter and Event Callbacks Combined](#test-2930-rust-filter-and-event-callbacks-combined) | `tests/e2e_callback.rs` | 2.9, 7.6, 12.5, 12.6 |
| 29.31 | [If-Match Single Deletion](#test-2931-if-match-single-deletion) | `tests/e2e_optimistic.rs` | 11.1, 11.3 |
| 29.32 | [If-Match Batch Deletion](#test-2932-if-match-batch-deletion) | `tests/e2e_optimistic.rs` | 11.1, 11.4 |
| 29.32a | [If-Match ETag Mismatch](#test-2932a-if-match-etag-mismatch-skips-modified-objects) | `tests/e2e_optimistic.rs` | 11.2 |
| 29.33 | [Worker Size Configuration](#test-2933-worker-size-configuration) | `tests/e2e_performance.rs` | 1.3, 1.4 |
| 29.34 | [Rate Limit Objects](#test-2934-rate-limit-objects) | `tests/e2e_performance.rs` | 8.7, 8.8 |
| 29.35 | [Max Parallel Listings](#test-2935-max-parallel-listings) | `tests/e2e_performance.rs` | 1.5, 1.6, 1.7 |
| 29.36 | [Object Listing Queue Size](#test-2936-object-listing-queue-size) | `tests/e2e_performance.rs` | 1.8 |
| 29.37 | [Max Keys Listing](#test-2937-max-keys-listing) | `tests/e2e_performance.rs` | 1.5 |
| 29.38 | [Verbosity Levels](#test-2938-verbosity-levels) | `tests/e2e_tracing.rs` | 4.1, 4.2, 4.3, 4.4, 4.5 |
| 29.39 | [JSON Tracing](#test-2939-json-tracing) | `tests/e2e_tracing.rs` | 4.7, 13.3 |
| 29.40 | [Quiet Mode](#test-2940-quiet-mode) | `tests/e2e_tracing.rs` | 7.4 |
| 29.41 | [Show No Progress](#test-2941-show-no-progress) | `tests/e2e_tracing.rs` | 7.4 |
| 29.42 | [Disable Color Tracing](#test-2942-disable-color-tracing) | `tests/e2e_tracing.rs` | 4.8, 4.9 |
| 29.43 | [Log Deletion Summary](#test-2943-log-deletion-summary) | `tests/e2e_tracing.rs` | 7.3 |
| 29.44 | [AWS SDK Tracing and Span Events](#test-2944-aws-sdk-tracing-and-span-events) | `tests/e2e_tracing.rs` | 4.5 |
| 29.45 | [Retry Options](#test-2945-retry-options) | `tests/e2e_retry.rs` | 6.1, 6.2 |
| 29.46 | [Timeout Options](#test-2946-timeout-options) | `tests/e2e_retry.rs` | 8.3 |
| 29.47 | [Disable Stalled Stream Protection](#test-2947-disable-stalled-stream-protection) | `tests/e2e_retry.rs` | 8.3 |
| 29.48 | [Access Denied Invalid Credentials](#test-2948-access-denied-invalid-credentials) | `tests/e2e_error.rs` | 4.10, 6.4, 10.5, 13.4 |
| 29.49 | [Nonexistent Bucket Error](#test-2949-nonexistent-bucket-error) | `tests/e2e_error.rs` | 6.4, 10.5 |
| 29.49a | [Batch Partial Failure Access Denied](#test-2949a-batch-partial-failure-access-denied) | `tests/e2e_error.rs` | 1.9, 6.4, 6.5 |
| 29.50 | [Warn As Error](#test-2950-warn-as-error) | `tests/e2e_error.rs` | 10.5 |
| 29.51 | [Target Region Override](#test-2951-target-region-override) | `tests/e2e_aws_config.rs` | 8.5 |
| 29.52 | [Target Force Path Style](#test-2952-target-force-path-style) | `tests/e2e_aws_config.rs` | 8.6 |
| 29.53 | [Target Request Payer](#test-2953-target-request-payer) | `tests/e2e_aws_config.rs` | 8.3 |
| 29.53a | [Allow Lua Unsafe VM](#test-2953a-allow-lua-unsafe-vm) | `tests/e2e_aws_config.rs` | 2.14 |
| 29.54 | [Multiple Filters Combined](#test-2954-multiple-filters-combined) | `tests/e2e_combined.rs` | 2.11 |
| 29.55 | [Dry Run with Delete All Versions](#test-2955-dry-run-with-delete-all-versions) | `tests/e2e_combined.rs` | 3.1, 5.2, 5.4 |
| 29.56 | [Dry Run with Filters](#test-2956-dry-run-with-filters) | `tests/e2e_combined.rs` | 3.1, 2.2 |
| 29.57 | [Max Delete with Filters](#test-2957-max-delete-with-filters) | `tests/e2e_combined.rs` | 3.6, 2.2 |
| 29.58 | [Lua Filter with Event Callback](#test-2958-lua-filter-with-event-callback) | `tests/e2e_combined.rs` | 2.8, 2.12, 7.6 |
| 29.59 | [Large Object Count](#test-2959-large-object-count) | `tests/e2e_combined.rs` | 1.1, 1.8 |
| 29.59a | [All Filters Combined](#test-2959a-all-filters-combined) | `tests/e2e_combined.rs` | 2.2, 2.3, 2.4, 2.5, 2.6, 2.11 |
| 29.60 | [Deletion Stats Accuracy](#test-2960-deletion-stats-accuracy) | `tests/e2e_stats.rs` | 6.5, 7.1, 7.3 |
| 29.61 | [Event Callback Receives All Event Types](#test-2961-event-callback-receives-all-event-types) | `tests/e2e_stats.rs` | 7.6, 7.7 |

## Test Execution

```bash
# Configure AWS credentials for the e2e test profile
aws configure --profile s3rm-e2e-test

# Run all E2E tests
RUSTFLAGS="--cfg e2e_test" cargo test --all-features --test '*' -- --test-threads=8

# Run a specific E2E test
RUSTFLAGS="--cfg e2e_test" cargo test --all-features --test e2e_basic -- e2e_basic_prefix_deletion
```

## Test File Organization

```
tests/
+-- common/
|   +-- mod.rs             # TestHelper, bucket management, pipeline runner
+-- e2e_filter.rs          # Tests 29.1-29.12 (filtering)
+-- e2e_deletion.rs        # Tests 29.13-29.17 (core deletion modes)
+-- e2e_safety.rs          # Tests 29.18-29.20 (safety features)
+-- e2e_versioning.rs      # Tests 29.21-29.23 (versioning)
+-- e2e_callback.rs        # Tests 29.24-29.30 (Lua + Rust callbacks)
+-- e2e_optimistic.rs      # Tests 29.31-29.32a (if-match)
+-- e2e_performance.rs     # Tests 29.33-29.37 (performance options)
+-- e2e_tracing.rs         # Tests 29.38-29.44 (logging/tracing)
+-- e2e_retry.rs           # Tests 29.45-29.47 (retry/timeout options)
+-- e2e_error.rs           # Tests 29.48-29.50 (error handling, access denial)
+-- e2e_aws_config.rs      # Tests 29.51-29.53a (AWS config options)
+-- e2e_combined.rs        # Tests 29.54-29.59 (combined/advanced scenarios)
+-- e2e_stats.rs           # Tests 29.60-29.61 (statistics verification)
```

## Notes

- All tests use `#![cfg(e2e_test)]` attribute (compiled only with `RUSTFLAGS="--cfg e2e_test"`)
- All tests use the AWS profile `s3rm-e2e-test` (via `--target-profile s3rm-e2e-test`)
- Each test uses a unique randomly generated bucket (`s3rm-e2e-{uuid}`)
- Post-processing always runs regardless of test success/failure
- Maximum 1000 objects per E2E test case
- These tests create and delete real AWS S3 resources

---

## Test 29.0: Infrastructure

**Test ID**: 29.0
**Function Name**: N/A (shared test helper module)
**File**: `tests/common/mod.rs`
**Requirements Validated**: -

### Description

Create the shared test helper module that all E2E tests depend on. This module provides the `TestHelper` struct and utility functions for bucket management, object operations, configuration building, and pipeline execution.

### Setup

No test-specific setup. This module is imported by all other E2E test files.

### Configuration

N/A -- this is infrastructure, not a test case.

### Expected Outcome

The module provides the following components:

- `TestHelper` struct wrapping an S3 `Client` built with profile `s3rm-e2e-test`
- `create_bucket(bucket)` -- creates a standard S3 bucket
- `create_versioned_bucket(bucket)` -- creates bucket and enables versioning
- `delete_bucket_cascade(bucket)` -- deletes all objects/versions and deletes the bucket (always runs in cleanup)
- `put_object(bucket, key, body)` -- uploads an object with given body bytes
- `put_object_with_content_type(bucket, key, body, content_type)` -- uploads with explicit content type
- `put_object_with_metadata(bucket, key, body, metadata: HashMap)` -- uploads with user-defined metadata
- `put_object_with_tags(bucket, key, body, tags: HashMap)` -- uploads with tagging
- `list_objects(bucket, prefix) -> Vec<String>` -- lists remaining object keys (for assertions)
- `list_object_versions(bucket) -> Vec<(String, String)>` -- lists version IDs (for versioning assertions)
- `count_objects(bucket, prefix) -> usize` -- counts objects remaining after deletion
- `generate_bucket_name() -> String` -- returns `s3rm-e2e-{uuid}` for test isolation
- `build_config(args: Vec<&str>) -> Config` -- calls `build_config_from_args` with common defaults prepended
- `run_pipeline(config: Config) -> PipelineResult` -- creates token, runs pipeline, returns stats and error state
- `init_tracing()` -- initializes a dummy tracing subscriber for test output
- `PipelineResult` struct with fields: `stats: DeletionStats`, `has_error: bool`, `has_warning: bool`, `errors: Vec<String>`
- Region constant: use region from the `s3rm-e2e-test` profile (e.g., `us-east-1`)
- All helper methods are `async` and use `tokio`

---

## Test 29.1: Include Regex Filter

**Test ID**: 29.1
**Function Name**: `e2e_filter_include_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.2

### Description

Tests the `--filter-include-regex` CLI option. Verifies that only objects whose keys match the provided regular expression are deleted, while non-matching objects remain untouched.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with keys matching `^logs/.*\.txt$` (e.g., `logs/app1.txt`, `logs/app2.txt`, ...)
   - 10 objects with keys like `data/file1.csv`, `data/file2.csv`, ...

### Configuration

```
s3://{bucket}/ --filter-include-regex "^logs/.*\\.txt$" --force
```

### Expected Outcome

- Only the 10 `logs/*.txt` objects are deleted
- All 10 `data/` objects remain in the bucket
- Stats show 10 deleted, 0 failed
- `count_objects(bucket, "data/")` returns 10
- `count_objects(bucket, "logs/")` returns 0

---

## Test 29.2: Exclude Regex Filter

**Test ID**: 29.2
**Function Name**: `e2e_filter_exclude_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.2

### Description

Tests the `--filter-exclude-regex` CLI option. Verifies that objects whose keys match the exclusion pattern are preserved, while all other objects are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with keys matching `^keep/` (e.g., `keep/file1.dat`, `keep/file2.dat`, ...)
   - 10 objects with keys like `delete/file1.dat`, `delete/file2.dat`, ...

### Configuration

```
s3://{bucket}/ --filter-exclude-regex "^keep/" --force
```

### Expected Outcome

- 10 `delete/` objects are deleted
- 10 `keep/` objects remain in the bucket
- Stats show 10 deleted, 0 failed
- `count_objects(bucket, "keep/")` returns 10
- `count_objects(bucket, "delete/")` returns 0

---

## Test 29.3: Include Content Type Regex Filter

**Test ID**: 29.3
**Function Name**: `e2e_filter_include_content_type_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.3

### Description

Tests the `--filter-include-content-type-regex` CLI option. Verifies that only objects whose Content-Type matches the provided regular expression are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with `Content-Type: text/plain`
   - 10 objects with `Content-Type: application/json`

### Configuration

```
s3://{bucket}/ --filter-include-content-type-regex "text/plain" --force
```

### Expected Outcome

- 10 `text/plain` objects are deleted
- 10 `application/json` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.4: Exclude Content Type Regex Filter

**Test ID**: 29.4
**Function Name**: `e2e_filter_exclude_content_type_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.3

### Description

Tests the `--filter-exclude-content-type-regex` CLI option. Verifies that objects whose Content-Type matches the exclusion pattern are preserved, while all other objects are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with `Content-Type: image/png`
   - 10 objects with `Content-Type: text/html`

### Configuration

```
s3://{bucket}/ --filter-exclude-content-type-regex "image/png" --force
```

### Expected Outcome

- 10 `text/html` objects are deleted
- 10 `image/png` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.5: Include Metadata Regex Filter

**Test ID**: 29.5
**Function Name**: `e2e_filter_include_metadata_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.4

### Description

Tests the `--filter-include-metadata-regex` CLI option. Verifies that only objects whose user-defined metadata matches the provided regular expression are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with metadata `env=production`
   - 10 objects with metadata `env=staging`

### Configuration

```
s3://{bucket}/ --filter-include-metadata-regex "env=production" --force
```

### Expected Outcome

- 10 `env=production` objects are deleted
- 10 `env=staging` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.6: Exclude Metadata Regex Filter

**Test ID**: 29.6
**Function Name**: `e2e_filter_exclude_metadata_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.4

### Description

Tests the `--filter-exclude-metadata-regex` CLI option. Verifies that objects whose user-defined metadata matches the exclusion pattern are preserved, while all other objects are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with metadata `tier=archive`
   - 10 objects with metadata `tier=hot`

### Configuration

```
s3://{bucket}/ --filter-exclude-metadata-regex "tier=archive" --force
```

### Expected Outcome

- 10 `tier=hot` objects are deleted
- 10 `tier=archive` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.6a: Include Metadata Regex Multiple Entries

**Test ID**: 29.6a
**Function Name**: `e2e_filter_include_metadata_regex_multiple_entries`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.4

### Description

Tests the `--filter-include-metadata-regex` CLI option with objects that have 3 or more metadata entries. Metadata entries are serialized sorted alphabetically by key and comma-separated (e.g., `env=production,team=backend,version=v2`). The regex must match across the sorted representation.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects with metadata `{env=production, team=backend, version=v2}` (serialized: `env=production,team=backend,version=v2`)
   - 10 objects with metadata `{env=staging, team=frontend, version=v1}` (serialized: `env=staging,team=frontend,version=v1`)

### Configuration

```
s3://{bucket}/ --filter-include-metadata-regex "env=production.*version=v2" --force
```

### Expected Outcome

- 10 objects with `env=production` metadata are deleted (regex spans sorted entries)
- 10 objects with `env=staging` metadata remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.6b: Exclude Metadata Regex Multiple Entries Alternation

**Test ID**: 29.6b
**Function Name**: `e2e_filter_exclude_metadata_regex_multiple_entries_alternation`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.4

### Description

Verifies `--filter-exclude-metadata-regex` with the spec's alternation pattern `"key1=(value1|value2),key2=value2"` correctly excludes objects whose sorted, comma-separated metadata matches. This tests the documented regex format from the CLI help.

### Setup

1. Create an S3 bucket
2. Upload 30 objects:
   - 10 under `prod/` with metadata `{env=production, team=backend, version=v2}` (sorted: `env=production,team=backend,version=v2`)
   - 10 under `staging/` with metadata `{env=staging, team=backend, version=v2}` (sorted: `env=staging,team=backend,version=v2`)
   - 10 under `dev/` with metadata `{env=development, team=frontend, version=v1}` (sorted: `env=development,team=frontend,version=v1`)

### Execution

```bash
s3rm s3://<bucket>/ --filter-exclude-metadata-regex "env=(production|staging),team=backend" --force
```

### Verification

- Pipeline completes without error
- 10 `dev/` objects deleted (not matched by exclude regex)
- 10 `prod/` objects remain (excluded by alternation matching `production`)
- 10 `staging/` objects remain (excluded by alternation matching `staging`)
- Stats show 10 deleted, 0 failed

### Expected Outcome

- Alternation pattern `(production|staging)` correctly matches both values in the exclude regex
- Only objects NOT matching the exclude pattern are deleted
- Comma separator between metadata keys is correctly interpreted

---

## Test 29.7: Include Tag Regex Filter

**Test ID**: 29.7
**Function Name**: `e2e_filter_include_tag_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.5

### Description

Tests the `--filter-include-tag-regex` CLI option. Verifies that only objects whose tags match the provided regular expression are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects tagged `status=expired`
   - 10 objects tagged `status=active`

### Configuration

```
s3://{bucket}/ --filter-include-tag-regex "status=expired" --force
```

### Expected Outcome

- 10 `status=expired` objects are deleted
- 10 `status=active` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.8: Exclude Tag Regex Filter

**Test ID**: 29.8
**Function Name**: `e2e_filter_exclude_tag_regex`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.5

### Description

Tests the `--filter-exclude-tag-regex` CLI option. Verifies that objects whose tags match the exclusion pattern are preserved, while all other objects are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects tagged `retain=true`
   - 10 objects tagged `retain=false`

### Configuration

```
s3://{bucket}/ --filter-exclude-tag-regex "retain=true" --force
```

### Expected Outcome

- 10 `retain=false` objects are deleted
- 10 `retain=true` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.8a: Include Tag Regex Multiple Tags

**Test ID**: 29.8a
**Function Name**: `e2e_filter_include_tag_regex_multiple_tags`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.5

### Description

Tests the `--filter-include-tag-regex` CLI option with objects that have 3 or more tags. Tags are serialized sorted alphabetically by key and `&`-separated (e.g., `env=production&retain=false&team=backend`). The regex must match across the sorted representation.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects tagged `{env=production, retain=false, team=backend}` (serialized: `env=production&retain=false&team=backend`)
   - 10 objects tagged `{env=staging, retain=true, team=frontend}` (serialized: `env=staging&retain=true&team=frontend`)

### Configuration

```
s3://{bucket}/ --filter-include-tag-regex "env=production.*retain=false" --force
```

### Expected Outcome

- 10 objects with `env=production` tags are deleted (regex spans sorted tag entries)
- 10 objects with `env=staging` tags remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.8b: Exclude Tag Regex Multiple Tags Alternation

**Test ID**: 29.8b
**Function Name**: `e2e_filter_exclude_tag_regex_multiple_tags_alternation`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.5

### Description

Verifies `--filter-exclude-tag-regex` with the spec's alternation pattern `"key1=(value1|value2)&key2=value2"` correctly excludes objects whose sorted, `&`-separated tags match. This tests the documented regex format from the CLI help.

### Setup

1. Create an S3 bucket
2. Upload 30 objects:
   - 10 under `prod/` tagged `{env=production, retain=true, team=backend}` (sorted: `env=production&retain=true&team=backend`)
   - 10 under `staging/` tagged `{env=staging, retain=true, team=backend}` (sorted: `env=staging&retain=true&team=backend`)
   - 10 under `dev/` tagged `{env=development, retain=false, team=frontend}` (sorted: `env=development&retain=false&team=frontend`)

### Execution

```bash
s3rm s3://<bucket>/ --filter-exclude-tag-regex "env=(production|staging)&retain=true" --force
```

### Verification

- Pipeline completes without error
- 10 `dev/` objects deleted (not matched by exclude regex)
- 10 `prod/` objects remain (excluded by alternation matching `production`)
- 10 `staging/` objects remain (excluded by alternation matching `staging`)
- Stats show 10 deleted, 0 failed

### Expected Outcome

- Alternation pattern `(production|staging)` correctly matches both values in the exclude regex
- Only objects NOT matching the exclude pattern are deleted
- `&` separator between tag keys is correctly interpreted

---

## Test 29.9: Mtime Before Filter

**Test ID**: 29.9
**Function Name**: `e2e_filter_mtime_before`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.7

### Description

Tests the `--filter-mtime-before` CLI option. Verifies that only objects with a last-modified time before the specified timestamp are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects (first batch -- older objects)
4. Record timestamp `T` (sleep briefly to ensure time separation)
5. Upload 10 more objects (second batch -- newer objects)

### Configuration

```
s3://{bucket}/ --filter-mtime-before <T in RFC 3339> --force
```

### Expected Outcome

- Only the first 10 (older) objects are deleted
- The newer 10 objects remain in the bucket
- Stats show 10 deleted, 0 failed
- Note: A sleep or known timestamp between batches is needed to reliably separate them

---

## Test 29.10: Mtime After Filter

**Test ID**: 29.10
**Function Name**: `e2e_filter_mtime_after`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.7

### Description

Tests the `--filter-mtime-after` CLI option. Verifies that only objects with a last-modified time after the specified timestamp are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects (first batch -- older objects)
4. Record timestamp `T` (sleep briefly to ensure time separation)
5. Upload 10 more objects (second batch -- newer objects)

### Configuration

```
s3://{bucket}/ --filter-mtime-after <T in RFC 3339> --force
```

### Expected Outcome

- Only the second 10 (newer) objects are deleted
- The older 10 objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.11: Smaller Size Filter

**Test ID**: 29.11
**Function Name**: `e2e_filter_smaller_size`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.6

### Description

Tests the `--filter-smaller-size` CLI option. Verifies that only objects smaller than the specified size threshold are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects of 100 bytes each (small objects)
   - 10 objects of 10KB each (large objects)

### Configuration

```
s3://{bucket}/ --filter-smaller-size 1KB --force
```

### Expected Outcome

- 10 small (100B) objects are deleted
- 10 large (10KB) objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.12: Larger Size Filter

**Test ID**: 29.12
**Function Name**: `e2e_filter_larger_size`
**File**: `tests/e2e_filter.rs`
**Requirements Validated**: 2.6

### Description

Tests the `--filter-larger-size` CLI option. Verifies that only objects larger than the specified size threshold are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects of 100 bytes each (small objects)
   - 10 objects of 10KB each (large objects)

### Configuration

```
s3://{bucket}/ --filter-larger-size 1KB --force
```

### Expected Outcome

- 10 large (10KB) objects are deleted
- 10 small (100B) objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.13: Basic Prefix Deletion

**Test ID**: 29.13
**Function Name**: `e2e_basic_prefix_deletion`
**File**: `tests/e2e_deletion.rs`
**Requirements Validated**: 1.1, 2.1

### Description

Tests basic deletion by prefix using the positional `target` argument. Verifies that all objects under the specified prefix are deleted while objects under other prefixes remain untouched.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 40 objects:
   - 30 objects under prefix `data/` (e.g., `data/file1.dat`, `data/file2.dat`, ...)
   - 10 objects under prefix `other/` (e.g., `other/file1.dat`, `other/file2.dat`, ...)

### Configuration

```
s3://{bucket}/data/ --force
```

### Expected Outcome

- All 30 `data/` objects are deleted
- 10 `other/` objects remain in the bucket
- Stats show 30 deleted, 0 failed
- `count_objects(bucket, "data/")` returns 0
- `count_objects(bucket, "other/")` returns 10

---

## Test 29.14: Batch Deletion Mode

**Test ID**: 29.14
**Function Name**: `e2e_batch_deletion_mode`
**File**: `tests/e2e_deletion.rs`
**Requirements Validated**: 1.1, 1.9

### Description

Tests batch deletion with a custom batch size using the `--batch-size` option. Verifies that objects are deleted in batches using the S3 DeleteObjects API and that all objects are removed successfully.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 500 objects under a single prefix

### Configuration

```
s3://{bucket}/ --batch-size 100 --force
```

### Expected Outcome

- All 500 objects are deleted
- Stats show 500 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.15: Single Deletion Mode

**Test ID**: 29.15
**Function Name**: `e2e_single_deletion_mode`
**File**: `tests/e2e_deletion.rs`
**Requirements Validated**: 1.2

### Description

Tests single-object deletion mode by setting `--batch-size 1`. Verifies that objects are deleted one at a time using the S3 DeleteObject API.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects

### Configuration

```
s3://{bucket}/ --batch-size 1 --force
```

### Expected Outcome

- All 20 objects are deleted
- Stats show 20 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.16: Delete Entire Bucket Contents

**Test ID**: 29.16
**Function Name**: `e2e_delete_entire_bucket_contents`
**File**: `tests/e2e_deletion.rs`
**Requirements Validated**: 2.10

### Description

Tests deleting all objects in a bucket by specifying no prefix. Verifies that all objects regardless of prefix are removed when the target is the entire bucket.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 50 objects with varied prefixes:
   - Objects under `a/`, `b/`, `c/`, and root-level (no prefix)

### Configuration

```
s3://{bucket} --force
```

Note: No prefix specified -- this deletes everything in the bucket.

### Expected Outcome

- All 50 objects are deleted
- Bucket is empty
- Stats show 50 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.17: Empty Bucket No Error

**Test ID**: 29.17
**Function Name**: `e2e_empty_bucket_no_error`
**File**: `tests/e2e_deletion.rs`
**Requirements Validated**: 1.8

### Description

Tests that running the deletion pipeline against an empty bucket completes without error. Verifies graceful handling of the case where no objects exist.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket (no objects uploaded)

### Configuration

```
s3://{bucket} --force
```

### Expected Outcome

- Pipeline completes without error
- `has_error` is false
- Stats show 0 deleted, 0 failed

---

## Test 29.18: Dry Run No Deletion

**Test ID**: 29.18
**Function Name**: `e2e_dry_run_no_deletion`
**File**: `tests/e2e_safety.rs`
**Requirements Validated**: 3.1

### Description

Tests the `--dry-run` flag to verify it prevents actual deletions. The full pipeline (listing, filtering) runs but deletions are simulated without making actual S3 API calls.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects

### Configuration

```
s3://{bucket}/ --dry-run --force
```

### Expected Outcome

- All 20 objects still exist in the bucket after pipeline completes
- Stats show 20 "deleted" (simulated)
- No actual S3 deletion API calls were made
- `count_objects(bucket, "")` returns 20

---

## Test 29.19: Max Delete Threshold

**Test ID**: 29.19
**Function Name**: `e2e_max_delete_threshold`
**File**: `tests/e2e_safety.rs`
**Requirements Validated**: 3.6

### Description

Tests the `--max-delete` option which stops deletion after the specified threshold is reached. Verifies that the pipeline cancels at deletion time when the count exceeds the limit.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 50 objects

### Configuration

```
s3://{bucket}/ --max-delete 10 --force
```

### Expected Outcome

- Exactly 10 objects are deleted
- 40 objects remain in the bucket
- Pipeline reports cancellation or partial completion
- `count_objects(bucket, "")` returns 40

---

## Test 29.20: Force Flag Skips Confirmation

**Test ID**: 29.20
**Function Name**: `e2e_force_flag_skips_confirmation`
**File**: `tests/e2e_safety.rs`
**Requirements Validated**: 3.4, 13.2

### Description

Tests the `--force` flag to verify that the pipeline runs without requiring a confirmation prompt. This confirms that `force=true` works correctly in the library API context.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --force
```

### Expected Outcome

- All 10 objects are deleted
- No prompt interaction required
- Pipeline completes successfully
- Stats show 10 deleted, 0 failed

---

## Test 29.21: Versioned Bucket Creates Delete Markers

**Test ID**: 29.21
**Function Name**: `e2e_versioned_bucket_creates_delete_markers`
**File**: `tests/e2e_versioning.rs`
**Requirements Validated**: 5.1

### Description

Tests that deleting from a versioned bucket without `--delete-all-versions` creates delete markers instead of permanently removing objects. Original versions should still exist.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a versioned bucket using `create_versioned_bucket()`
3. Upload 10 objects (creates initial versions)

### Configuration

```
s3://{bucket}/ --force
```

Note: No `--delete-all-versions` flag.

### Expected Outcome

- Delete markers are created for each object
- Original versions still exist in the bucket
- `list_object_versions(bucket)` shows both delete markers and original versions
- The objects are no longer visible via standard `list_objects` (hidden by delete markers)

---

## Test 29.22: Delete All Versions

**Test ID**: 29.22
**Function Name**: `e2e_delete_all_versions`
**File**: `tests/e2e_versioning.rs`
**Requirements Validated**: 5.2, 5.4

### Description

Tests the `--delete-all-versions` flag to verify it deletes all versions of matching objects including delete markers. Each version and marker counts as a separate deletion in statistics.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a versioned bucket using `create_versioned_bucket()`
3. Upload 10 objects (creates initial versions)
4. Overwrite each object once (creates 2 versions each = 20 versions total)
5. Delete some objects via standard S3 API (creates delete markers)

### Configuration

```
s3://{bucket}/ --delete-all-versions --force
```

### Expected Outcome

- No object versions or delete markers remain
- Bucket is completely clean
- `list_object_versions(bucket)` returns empty
- Stats count each version/marker as a separate deletion

---

## Test 29.23: Delete All Versions Unversioned Bucket Error

**Test ID**: 29.23
**Function Name**: `e2e_delete_all_versions_unversioned_bucket_error`
**File**: `tests/e2e_versioning.rs`
**Requirements Validated**: 5.2

### Description

Tests that using `--delete-all-versions` on a non-versioned bucket produces an error. The pipeline should detect the versioning requirement is not met.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard (non-versioned) bucket
3. Upload 5 objects

### Configuration

```
s3://{bucket}/ --delete-all-versions --force
```

### Expected Outcome

- Pipeline reports error about versioning requirement
- No objects are deleted
- `has_error` is true
- `count_objects(bucket, "")` returns 5

---

## Test 29.24: Rust Filter Callback

**Test ID**: 29.24
**Function Name**: `e2e_rust_filter_callback`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 2.9, 12.5

### Description

Tests registering a Rust `FilterCallback` via `config.filter_manager.register_callback()`. Verifies that the filter callback controls which objects are deleted based on custom Rust logic.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 with keys starting with `keep-`
   - 10 with keys starting with `delete-`

### Configuration

1. Build config with `--force`
2. Register a Rust filter callback that returns `true` only for keys starting with `delete-`

### Expected Outcome

- 10 `delete-` objects are removed
- 10 `keep-` objects remain in the bucket
- Stats show 10 deleted, 0 failed
- `count_objects(bucket, "keep-")` returns 10
- `count_objects(bucket, "delete-")` returns 0

---

## Test 29.25: Rust Event Callback

**Test ID**: 29.25
**Function Name**: `e2e_rust_event_callback`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 7.6, 7.7, 12.6

### Description

Tests registering a Rust `EventCallback` via `config.event_manager.register_callback()`. Verifies that the event callback receives all expected event types with correct data.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

1. Build config with `--force`
2. Register a Rust event callback with `EventType::ALL_EVENTS` that collects events into an `Arc<Mutex<Vec<EventData>>>`

### Expected Outcome

- All 10 objects are deleted
- Callback received `PIPELINE_START` event
- Callback received `DELETE_COMPLETE` event 10 times
- Callback received `PIPELINE_END` event
- Event data includes correct keys and sizes

---

## Test 29.26: Lua Filter Callback

**Test ID**: 29.26
**Function Name**: `e2e_lua_filter_callback`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 2.8, 2.12

### Description

Tests the `--filter-callback-lua-script` option with a Lua filter script. Verifies that the Lua callback determines which objects are deleted based on custom scripting logic.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 with `.tmp` extension
   - 10 with `.dat` extension
4. Write a temporary Lua filter script that returns `true` for keys ending in `.tmp`

### Configuration

```
s3://{bucket}/ --filter-callback-lua-script <path_to_lua_script> --force
```

### Expected Outcome

- 10 `.tmp` objects are deleted
- 10 `.dat` objects remain in the bucket
- Stats show 10 deleted, 0 failed

---

## Test 29.27: Lua Event Callback

**Test ID**: 29.27
**Function Name**: `e2e_lua_event_callback`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 2.12, 7.6

### Description

Tests the `--event-callback-lua-script` option with a Lua event script. Verifies that the Lua event callback receives events during the deletion operation.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects
4. Write a temporary Lua event script that writes event counts to a temp file (using `allow_lua_os_library`)

### Configuration

```
s3://{bucket}/ --event-callback-lua-script <path_to_lua_script> --allow-lua-os-library --force
```

### Expected Outcome

- All 10 objects are deleted
- The Lua output file contains expected event records
- Events include deletion completion entries

---

## Test 29.28: Lua Sandbox Blocks OS Access

**Test ID**: 29.28
**Function Name**: `e2e_lua_sandbox_blocks_os_access`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 2.13

### Description

Tests that the default Lua sandbox blocks OS library access. When a Lua filter script attempts to call `os.execute()`, the pipeline should report a sandbox violation error.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects
4. Write a Lua filter script that calls `os.execute("echo test")`

### Configuration

```
s3://{bucket}/ --filter-callback-lua-script <path_to_lua_script> --force
```

Note: No `--allow-lua-os-library` flag -- sandbox is enforced by default.

### Expected Outcome

- Pipeline reports error (Lua sandbox violation)
- Objects may or may not be deleted depending on when the error occurs
- The Lua script is prevented from executing OS commands

---

## Test 29.29: Lua VM Memory Limit

**Test ID**: 29.29
**Function Name**: `e2e_lua_vm_memory_limit`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 2.14

### Description

Tests the `--lua-vm-memory-limit` option to verify that Lua memory allocation is capped. When a Lua script attempts to allocate memory exceeding the limit, the pipeline should report an error.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects
4. Write a Lua filter script that allocates a large table (exceeding 1MB)

### Configuration

```
s3://{bucket}/ --filter-callback-lua-script <path_to_lua_script> --lua-vm-memory-limit 1MB --force
```

### Expected Outcome

- Pipeline reports error or process terminates due to Lua memory limit exceeded
- The Lua VM is prevented from consuming excessive memory

---

## Test 29.30: Rust Filter and Event Callbacks Combined

**Test ID**: 29.30
**Function Name**: `e2e_rust_filter_and_event_callbacks_combined`
**File**: `tests/e2e_callback.rs`
**Requirements Validated**: 2.9, 7.6, 12.5, 12.6

### Description

Tests using both Rust filter and event callbacks simultaneously. Verifies that filtering works correctly while event callbacks capture all relevant events including both deletion and filter-skip events.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 large objects (5KB each)
   - 10 small objects (100B each)

### Configuration

1. Build config with `--force`
2. Register a Rust filter callback that deletes only objects >= 1KB
3. Register a Rust event callback collecting events into `Arc<Mutex<Vec<EventData>>>`

### Expected Outcome

- 10 large objects are deleted
- 10 small objects remain in the bucket
- Event callback received `DELETE_COMPLETE` for each deleted object
- Event callback received `DELETE_FILTERED` for each skipped (filtered-out) object
- Stats show 10 deleted, 0 failed

---

## Test 29.31: If-Match Single Deletion

**Test ID**: 29.31
**Function Name**: `e2e_if_match_single_deletion`
**File**: `tests/e2e_optimistic.rs`
**Requirements Validated**: 11.1, 11.3

### Description

Tests the `--if-match` flag with single deletion mode (`--batch-size 1`). Verifies that optimistic locking via ETag matching works correctly when objects have not been modified between listing and deletion.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --if-match --batch-size 1 --force
```

### Expected Outcome

- All 10 objects are deleted (ETags match since objects were not modified between list and delete)
- Stats show 10 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.32: If-Match Batch Deletion

**Test ID**: 29.32
**Function Name**: `e2e_if_match_batch_deletion`
**File**: `tests/e2e_optimistic.rs`
**Requirements Validated**: 11.1, 11.4

### Description

Tests the `--if-match` flag with batch deletion mode. Verifies that optimistic locking via ETag matching works correctly with the S3 DeleteObjects batch API.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects

### Configuration

```
s3://{bucket}/ --if-match --batch-size 10 --force
```

### Expected Outcome

- All 20 objects are deleted (ETags match for unmodified objects)
- Stats show 20 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.32a: If-Match ETag Mismatch Skips Modified Objects

**Test ID**: 29.32a
**Function Name**: `e2e_if_match_etag_mismatch_skips_modified_objects`
**File**: `tests/e2e_optimistic.rs`
**Requirements Validated**: 11.2

### Description

Tests that the `--if-match` flag correctly handles ETag mismatches. When an object is modified between the listing phase and the deletion phase, its ETag changes. The If-Match condition should fail for these modified objects, causing them to be skipped rather than deleted. This validates the core safety property of optimistic locking.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects with known keys
4. Register a Rust filter callback that:
   - On its first invocation, overwrites 3 specific objects with new content using the AWS SDK (changing their ETags)
   - Returns `true` for all objects (allowing all to proceed to deletion)

### Configuration

```
s3://{bucket}/ --if-match --batch-size 1 --force
```

Single deletion mode (`--batch-size 1`) is used for clearer per-object behavior.

### Expected Outcome

- 7 unmodified objects are deleted (ETag matches)
- 3 modified objects remain in the bucket (ETag mismatch causes skip)
- Stats show 7 deleted
- `has_warning` is true or error count reflects the 3 ETag mismatches
- `count_objects(bucket, "")` returns 3
- The 3 modified objects still exist with their new content

---

## Test 29.33: Worker Size Configuration

**Test ID**: 29.33
**Function Name**: `e2e_worker_size_configuration`
**File**: `tests/e2e_performance.rs`
**Requirements Validated**: 1.3, 1.4

### Description

Tests the `--worker-size` option with a custom worker count. Verifies that configuring the number of concurrent workers does not affect correctness of deletion.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 100 objects

### Configuration

```
s3://{bucket}/ --worker-size 4 --force
```

### Expected Outcome

- All 100 objects are deleted
- Stats show 100 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.34: Rate Limit Objects

**Test ID**: 29.34
**Function Name**: `e2e_rate_limit_objects`
**File**: `tests/e2e_performance.rs`
**Requirements Validated**: 8.7, 8.8

### Description

Tests the `--rate-limit-objects` option for rate limiting deletion throughput. Verifies that rate limiting does not prevent completion of the deletion operation.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 50 objects

### Configuration

```
s3://{bucket}/ --rate-limit-objects 200 --force
```

Note: 200 objects/sec is slow but should complete the test.

### Expected Outcome

- All 50 objects are deleted
- Pipeline completes (rate limiting does not prevent completion)
- Stats show 50 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.35: Max Parallel Listings

**Test ID**: 29.35
**Function Name**: `e2e_max_parallel_listings`
**File**: `tests/e2e_performance.rs`
**Requirements Validated**: 1.5, 1.6, 1.7

### Description

Tests the `--max-parallel-listings` and `--max-parallel-listing-max-depth` options. Verifies that parallel listing configuration works correctly and does not cause errors.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 100 objects with varied prefixes (5 different top-level prefixes, 20 objects each)

### Configuration

```
s3://{bucket}/ --max-parallel-listings 2 --max-parallel-listing-max-depth 1 --force
```

### Expected Outcome

- All 100 objects are deleted
- Parallel listing configuration did not cause errors
- Stats show 100 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.36: Object Listing Queue Size

**Test ID**: 29.36
**Function Name**: `e2e_object_listing_queue_size`
**File**: `tests/e2e_performance.rs`
**Requirements Validated**: 1.8

### Description

Tests the `--object-listing-queue-size` option. Verifies that a small queue size does not prevent completion of the deletion pipeline.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 50 objects

### Configuration

```
s3://{bucket}/ --object-listing-queue-size 5 --force
```

### Expected Outcome

- All 50 objects are deleted (small queue size does not prevent completion)
- Stats show 50 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.37: Max Keys Listing

**Test ID**: 29.37
**Function Name**: `e2e_max_keys_listing`
**File**: `tests/e2e_performance.rs`
**Requirements Validated**: 1.5

### Description

Tests the `--max-keys` option which controls the number of objects returned per list request. Verifies that pagination with a small page size works correctly.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 100 objects

### Configuration

```
s3://{bucket}/ --max-keys 10 --force
```

Note: Forces pagination with 10 objects per page.

### Expected Outcome

- All 100 objects are deleted (pagination with small page size works correctly)
- Stats show 100 deleted, 0 failed
- `count_objects(bucket, "")` returns 0

---

## Test 29.38: Verbosity Levels

**Test ID**: 29.38
**Function Name**: `e2e_verbosity_levels`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 4.1, 4.2, 4.3, 4.4, 4.5

### Description

Tests the `-v`, `-vv`, and `-vvv` verbosity flags in a grouped test. Verifies that different verbosity levels do not affect functional correctness of the deletion pipeline.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects

### Configuration

Run 3 times with different verbosity levels, all with `--force`:
1. `s3://{bucket}/ -v --force`
2. `s3://{bucket}/ -vv --force`
3. `s3://{bucket}/ -vvv --force`

### Expected Outcome

- All 5 objects are deleted in each run
- No errors in any run
- Pipeline completes successfully regardless of verbosity level
- Logging configuration does not affect deletion functionality

---

## Test 29.39: JSON Tracing

**Test ID**: 29.39
**Function Name**: `e2e_json_tracing`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 4.7, 13.3

### Description

Tests the `--json-tracing` option which outputs structured JSON format for all log levels. Requires `--force` because JSON output is incompatible with interactive confirmation prompts.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects

### Configuration

```
s3://{bucket}/ --json-tracing --force
```

### Expected Outcome

- Pipeline completes successfully
- All 5 objects are deleted
- Stats show 5 deleted, 0 failed

---

## Test 29.40: Quiet Mode

**Test ID**: 29.40
**Function Name**: `e2e_quiet_mode`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 7.4

### Description

Tests the `-q` (quiet mode) option which suppresses progress output. Verifies that quiet mode does not affect functional correctness.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects

### Configuration

```
s3://{bucket}/ -q --force
```

### Expected Outcome

- Pipeline completes successfully
- All 5 objects are deleted
- Stats show 5 deleted, 0 failed

---

## Test 29.41: Show No Progress

**Test ID**: 29.41
**Function Name**: `e2e_show_no_progress`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 7.4

### Description

Tests the `--show-no-progress` option which hides the progress bar. Verifies that hiding progress display does not affect functional correctness.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects

### Configuration

```
s3://{bucket}/ --show-no-progress --force
```

### Expected Outcome

- Pipeline completes successfully
- All 5 objects are deleted
- Stats show 5 deleted, 0 failed

---

## Test 29.42: Disable Color Tracing

**Test ID**: 29.42
**Function Name**: `e2e_disable_color_tracing`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 4.8, 4.9

### Description

Tests the `--disable-color-tracing` option which disables colored output in log messages. Verifies that disabling color does not affect functional correctness.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects

### Configuration

```
s3://{bucket}/ --disable-color-tracing --force
```

### Expected Outcome

- Pipeline completes successfully
- All 5 objects are deleted
- Stats show 5 deleted, 0 failed

---

## Test 29.43: Log Deletion Summary

**Test ID**: 29.43
**Function Name**: `e2e_log_deletion_summary`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 7.3

### Description

Tests the `--log-deletion-summary` option which logs deletion summary statistics. Verifies that the summary logging option does not affect functional correctness.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --log-deletion-summary --force
```

### Expected Outcome

- Pipeline completes successfully
- All 10 objects are deleted
- Stats are logged in the output
- Stats show 10 deleted, 0 failed

---

## Test 29.44: AWS SDK Tracing and Span Events

**Test ID**: 29.44
**Function Name**: `e2e_aws_sdk_tracing_and_span_events`
**File**: `tests/e2e_tracing.rs`
**Requirements Validated**: 4.5

### Description

Tests the `--aws-sdk-tracing` and `--span-events-tracing` options together. Verifies that enabling AWS SDK trace events and span events does not affect functional correctness.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects

### Configuration

```
s3://{bucket}/ --aws-sdk-tracing --span-events-tracing -vvv --force
```

### Expected Outcome

- Pipeline completes successfully
- All 5 objects are deleted
- AWS SDK trace events are produced
- Stats show 5 deleted, 0 failed

---

## Test 29.45: Retry Options

**Test ID**: 29.45
**Function Name**: `e2e_retry_options`
**File**: `tests/e2e_retry.rs`
**Requirements Validated**: 6.1, 6.2

### Description

Tests the retry configuration options: `--aws-max-attempts`, `--initial-backoff-milliseconds`, `--force-retry-count`, and `--force-retry-interval-milliseconds`. Verifies that retry options do not prevent normal operation.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --aws-max-attempts 2 --initial-backoff-milliseconds 100 --force-retry-count 1 --force-retry-interval-milliseconds 100 --force
```

### Expected Outcome

- Pipeline completes successfully
- All 10 objects are deleted
- Retry options do not prevent normal operation
- Stats show 10 deleted, 0 failed

---

## Test 29.46: Timeout Options

**Test ID**: 29.46
**Function Name**: `e2e_timeout_options`
**File**: `tests/e2e_retry.rs`
**Requirements Validated**: 8.3

### Description

Tests the timeout configuration options: `--operation-timeout-milliseconds`, `--operation-attempt-timeout-milliseconds`, `--connect-timeout-milliseconds`, and `--read-timeout-milliseconds`. Verifies that timeout options allow normal operation when set to reasonable values.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --operation-timeout-milliseconds 30000 --connect-timeout-milliseconds 5000 --read-timeout-milliseconds 5000 --force
```

### Expected Outcome

- Pipeline completes within timeout
- All 10 objects are deleted
- Stats show 10 deleted, 0 failed

---

## Test 29.47: Disable Stalled Stream Protection

**Test ID**: 29.47
**Function Name**: `e2e_disable_stalled_stream_protection`
**File**: `tests/e2e_retry.rs`
**Requirements Validated**: 8.3

### Description

Tests the `--disable-stalled-stream-protection` option. Verifies that disabling stalled stream protection does not prevent normal operation.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --disable-stalled-stream-protection --force
```

### Expected Outcome

- Pipeline completes successfully
- All 10 objects are deleted
- Stats show 10 deleted, 0 failed

---

## Test 29.48: Access Denied Invalid Credentials

**Test ID**: 29.48
**Function Name**: `e2e_access_denied_invalid_credentials`
**File**: `tests/e2e_error.rs`
**Requirements Validated**: 4.10, 6.4, 10.5, 13.4

### Description

Tests access denial with invalid AWS credentials. Verifies that the pipeline properly reports errors when authentication fails.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard bucket and upload 5 objects (using valid credentials for setup)

### Configuration

```
s3://{bucket}/ --target-access-key INVALID --target-secret-access-key INVALID --target-region us-east-1 --force -v
```

### Expected Outcome

- Pipeline reports error
- `has_error()` returns true
- Error type is AWS SDK error (access denied or invalid credentials)
- Error message includes the error code (validates Req 4.10: failures logged with error message and code)
- Pipeline returns distinct exit code for authentication failure

---

## Test 29.49: Nonexistent Bucket Error

**Test ID**: 29.49
**Function Name**: `e2e_nonexistent_bucket_error`
**File**: `tests/e2e_error.rs`
**Requirements Validated**: 6.4, 10.5

### Description

Tests deleting from a bucket that does not exist. Verifies that the pipeline properly reports the error condition.

### Setup

No bucket creation needed. Use a random non-existent bucket name.

### Configuration

```
s3://s3rm-e2e-nonexistent-{uuid}/ --force
```

### Expected Outcome

- Pipeline reports error (NoSuchBucket or similar)
- `has_error()` returns true
- Error message indicates the bucket does not exist

---

## Test 29.49a: Batch Partial Failure Access Denied

**Test ID**: 29.49a
**Function Name**: `e2e_batch_partial_failure_access_denied`
**File**: `tests/e2e_error.rs`
**Requirements Validated**: 1.9, 6.4, 6.5

### Description

Tests that batch deletion handles partial failure correctly when some objects are protected by a bucket policy. A deny policy on `s3:DeleteObject` for a specific prefix causes those objects to fail with AccessDenied, while objects outside the prefix are deleted normally. The pipeline must report both successful deletions and failures accurately.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 objects under `deletable/` (no restrictions)
   - 10 objects under `protected/` (deletion will be denied)
4. Apply a bucket policy denying `s3:DeleteObject` on `protected/*`

### Configuration

```
s3://{bucket}/ --force
```

### Expected Outcome

- 10 `deletable/` objects are deleted
- 10 `protected/` objects remain in the bucket (AccessDenied)
- `stats.stats_deleted_objects == 10`
- `stats.stats_failed_objects > 0` (the 10 protected objects)
- Pipeline reports `has_warning` or `has_error` for the partial failure
- Deny policy is removed before cleanup so bucket can be fully cleaned up

---

## Test 29.50: Warn As Error

**Test ID**: 29.50
**Function Name**: `e2e_warn_as_error`
**File**: `tests/e2e_error.rs`
**Requirements Validated**: 10.5

### Description

Tests the `--warn-as-error` option which promotes warnings to errors. Verifies that when warnings occur during the pipeline, they are treated as errors in the final result.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --warn-as-error --force
```

### Expected Outcome

- Pipeline completes
- If any warnings occurred during execution, `has_error()` returns true
- If no warnings occurred, normal completion (success)
- All 10 objects are deleted in either case

---

## Test 29.51: Target Region Override

**Test ID**: 29.51
**Function Name**: `e2e_target_region_override`
**File**: `tests/e2e_aws_config.rs`
**Requirements Validated**: 8.5

### Description

Tests the `--target-region` option which overrides the region from the AWS profile. Verifies that explicitly specifying the region does not break bucket access.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects in the default region

### Configuration

```
s3://{bucket}/ --target-region <same-region-as-bucket> --force
```

### Expected Outcome

- Pipeline completes successfully
- All 10 objects are deleted
- Region override does not break access
- Stats show 10 deleted, 0 failed

---

## Test 29.52: Target Force Path Style

**Test ID**: 29.52
**Function Name**: `e2e_target_force_path_style`
**File**: `tests/e2e_aws_config.rs`
**Requirements Validated**: 8.6

### Description

Tests the `--target-force-path-style` option which forces path-style S3 access instead of virtual-hosted-style. Verifies that path-style access works with standard S3.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --target-force-path-style --force
```

### Expected Outcome

- Pipeline completes successfully
- All 10 objects are deleted
- Path-style access works with standard S3
- Stats show 10 deleted, 0 failed

---

## Test 29.53: Target Request Payer

**Test ID**: 29.53
**Function Name**: `e2e_target_request_payer`
**File**: `tests/e2e_aws_config.rs`
**Requirements Validated**: 8.3

### Description

Tests the `--target-request-payer` option which adds the request payer header to S3 API calls. Verifies that the request payer header does not break normal operations.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 10 objects

### Configuration

```
s3://{bucket}/ --target-request-payer --force
```

### Expected Outcome

- Pipeline completes successfully
- All 10 objects are deleted
- Request payer header does not break normal operations
- Stats show 10 deleted, 0 failed

---

## Test 29.53a: Allow Lua Unsafe VM

**Test ID**: 29.53a
**Function Name**: `e2e_allow_lua_unsafe_vm`
**File**: `tests/e2e_aws_config.rs`
**Requirements Validated**: 2.14

### Description

Tests the `--allow-lua-unsafe-vm` option which disables the Lua sandbox entirely. Verifies that when the unsafe VM mode is enabled, Lua scripts can access OS library functions without sandbox errors.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects
4. Write a Lua filter script that uses `os.clock()` (an OS library function)

### Configuration

```
s3://{bucket}/ --filter-callback-lua-script <path_to_lua_script> --allow-lua-unsafe-vm --force
```

### Expected Outcome

- Pipeline completes without Lua sandbox error
- All 5 matching objects are deleted
- Unsafe VM allows OS library access
- Stats show 5 deleted, 0 failed

---

## Test 29.54: Multiple Filters Combined

**Test ID**: 29.54
**Function Name**: `e2e_multiple_filters_combined`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 2.11

### Description

Tests multiple filters combined with AND logic. Verifies that when prefix, regex, and size filters are all specified, only objects matching ALL criteria are deleted.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 30 objects:
   - 10 with key `logs/small.txt` (100B each)
   - 10 with key `logs/large.txt` (10KB each)
   - 10 with key `data/large.dat` (10KB each)

### Configuration

```
s3://{bucket}/logs/ --filter-include-regex "\\.txt$" --filter-larger-size 1KB --force
```

### Expected Outcome

- Only 10 `logs/large.txt` objects are deleted (matching prefix AND regex AND size)
- 20 other objects remain in the bucket
- `count_objects(bucket, "logs/small")` returns 10 (too small)
- `count_objects(bucket, "data/")` returns 10 (wrong prefix)
- Stats show 10 deleted, 0 failed

---

## Test 29.55: Dry Run with Delete All Versions

**Test ID**: 29.55
**Function Name**: `e2e_dry_run_with_delete_all_versions`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 3.1, 5.2, 5.4

### Description

Tests dry-run mode combined with `--delete-all-versions`. Verifies that the simulation correctly counts all versions without actually performing any deletions.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a versioned bucket using `create_versioned_bucket()`
3. Upload 10 objects
4. Overwrite each object once (creates 20 versions total)

### Configuration

```
s3://{bucket}/ --dry-run --delete-all-versions --force
```

### Expected Outcome

- All 20 versions still exist in the bucket (dry-run prevented actual deletion)
- Stats show 20 "deleted" (simulated)
- No actual S3 deletion API calls were made
- `list_object_versions(bucket)` returns 20 versions

---

## Test 29.56: Dry Run with Filters

**Test ID**: 29.56
**Function Name**: `e2e_dry_run_with_filters`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 3.1, 2.2

### Description

Tests dry-run combined with filters to verify that the simulation accurately reflects which objects would be deleted by the filter criteria.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects:
   - 10 with keys matching `^temp/`
   - 10 with keys not matching `^temp/`

### Configuration

```
s3://{bucket}/ --dry-run --filter-include-regex "^temp/" --force
```

### Expected Outcome

- All 20 objects still exist in the bucket
- Stats show 10 "deleted" (the matching ones, simulated)
- No actual S3 deletion API calls were made
- `count_objects(bucket, "")` returns 20

---

## Test 29.57: Max Delete with Filters

**Test ID**: 29.57
**Function Name**: `e2e_max_delete_with_filters`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 3.6, 2.2

### Description

Tests the interaction between `--max-delete` and filters. Verifies that the deletion limit is applied after filtering, so only filtered-in objects count toward the max-delete threshold.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 50 objects:
   - 30 with keys matching `^to-delete/`
   - 20 with keys not matching `^to-delete/`

### Configuration

```
s3://{bucket}/ --filter-include-regex "^to-delete/" --max-delete 5 --force
```

### Expected Outcome

- Exactly 5 of the `to-delete/` objects are deleted
- 25 `to-delete/` objects remain (30 - 5)
- 20 other objects remain (not affected by filter)
- Total remaining: 45 objects

---

## Test 29.58: Lua Filter with Event Callback

**Test ID**: 29.58
**Function Name**: `e2e_lua_filter_with_event_callback`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 2.8, 2.12, 7.6

### Description

Tests using both a Lua filter callback and a Lua event callback simultaneously. Verifies that Lua-based filtering and event collection work correctly together.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects with varying sizes (some > 500B, some <= 500B)
4. Write a Lua filter script that deletes objects with size > 500B
5. Write a Lua event script that logs events to a file (requires `--allow-lua-os-library`)

### Configuration

```
s3://{bucket}/ --filter-callback-lua-script <filter_script> --event-callback-lua-script <event_script> --allow-lua-os-library --force
```

### Expected Outcome

- Only objects > 500B are deleted
- Objects <= 500B remain in the bucket
- Event script output file contains expected events (deletion completions)

---

## Test 29.59: Large Object Count

**Test ID**: 29.59
**Function Name**: `e2e_large_object_count`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 1.1, 1.8

### Description

Tests deleting close to the maximum allowed objects per test case (1000). Verifies that the pipeline handles a large number of objects correctly without memory issues or failures.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 1000 objects (each 1KB)

### Configuration

```
s3://{bucket}/ --force
```

### Expected Outcome

- All 1000 objects are deleted
- Stats show 1000 deleted, 0 failed
- `count_objects(bucket, "")` returns 0
- Pipeline handles the large count without memory issues

---

## Test 29.59a: All Filters Combined

**Test ID**: 29.59a
**Function Name**: `e2e_all_filters_combined`
**File**: `tests/e2e_combined.rs`
**Requirements Validated**: 2.2, 2.3, 2.4, 2.5, 2.6, 2.11

### Description

Tests that ALL available filter types combine with AND logic simultaneously. Uploads 20 objects with carefully varied properties (keys, sizes, content-types, metadata, tags) so that each filter independently eliminates specific objects. Only 3 objects pass every single filter and are deleted; the remaining 17 survive because each is excluded by at least one filter. This validates the complete filter pipeline end-to-end.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 20 objects across 8 groups:
   - **[1-3]** `data/report{i}.json` 1KB, `application/json`, metadata `env=production`, tags `deletable=yes` — **PASS ALL**
   - **[4-6]** `data/retained{i}.json` 1KB, `application/json`, metadata `env=production,retain=true`, tags `deletable=yes` — fail exclude-metadata
   - **[7-9]** `data/archive{i}.json` 1KB, `application/json`, metadata `env=production`, tags `deletable=yes` — fail exclude-regex
   - **[10-12]** `data/report{i}.csv` 1KB, `text/csv`, metadata `env=production`, tags `deletable=yes` — fail include-regex
   - **[13-15]** `data/tiny{i}.json` 100B, `application/json`, metadata `env=production`, tags `deletable=yes` — fail larger-size
   - **[16-17]** `data/staging{i}.json` 1KB, `application/json`, metadata `env=staging`, tags `deletable=yes` — fail include-metadata
   - **[18-19]** `data/guarded{i}.json` 1KB, `application/json`, metadata `env=production`, tags `protected=true` — fail exclude-tag
   - **[20]** `other/report0.json` 1KB, `application/json`, metadata `env=production`, tags `deletable=yes` — fail prefix

### Configuration

```
s3://{bucket}/data/ --filter-include-regex "\.json$" --filter-exclude-regex "archive" --filter-larger-size 500B --filter-smaller-size 5KB --filter-include-content-type-regex "application/json" --filter-include-metadata-regex "env=production" --filter-exclude-metadata-regex "retain=true" --filter-include-tag-regex "deletable=yes" --filter-exclude-tag-regex "protected=true" --force
```

### Expected Outcome

- Exactly 3 objects deleted (`data/report0.json`, `data/report1.json`, `data/report2.json`)
- 17 objects remain in the bucket
- Each excluded group verified individually (retained: 3, archive: 3, csv: 3, tiny: 3, staging: 2, guarded: 2, other: 1)
- Stats show 3 deleted, 0 failed

---

## Test 29.60: Deletion Stats Accuracy

**Test ID**: 29.60
**Function Name**: `e2e_deletion_stats_accuracy`
**File**: `tests/e2e_stats.rs`
**Requirements Validated**: 6.5, 7.1, 7.3

### Description

Tests that `get_deletion_stats()` returns accurate counts and byte totals. Verifies precise accounting of deleted objects, byte counts, and failure counts.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 15 objects of known sizes:
   - 5 objects x 1KB = 5KB
   - 5 objects x 2KB = 10KB
   - 5 objects x 5KB = 25KB
   - Total: 40KB (40960 bytes)

### Configuration

```
s3://{bucket}/ --force
```

### Expected Outcome

- `stats.stats_deleted_objects == 15`
- `stats.stats_deleted_bytes == 40960` (exact byte count)
- `stats.stats_failed_objects == 0`
- `stats.duration > 0`
- All 15 objects are deleted from the bucket

---

## Test 29.61: Event Callback Receives All Event Types

**Test ID**: 29.61
**Function Name**: `e2e_event_callback_receives_all_event_types`
**File**: `tests/e2e_stats.rs`
**Requirements Validated**: 7.6, 7.7

### Description

Tests that the event callback receives all expected event types during a deletion operation. Verifies that PIPELINE_START, DELETE_COMPLETE, STATS_REPORT, and PIPELINE_END events are all dispatched correctly.

### Setup

1. Generate a unique bucket name using `generate_bucket_name()`
2. Create a standard S3 bucket
3. Upload 5 objects
4. Register a Rust event callback collecting all events

### Configuration

```
s3://{bucket}/ --force
```

With a Rust event callback registered via the library API.

### Expected Outcome

- Received exactly 1 `PIPELINE_START` event
- Received 5 `DELETE_COMPLETE` events (one per object)
- Received at least 1 `PIPELINE_END` event
- `DELETE_COMPLETE` events contain key and size information
- `PIPELINE_END` event contains final stats
- All 5 objects are deleted from the bucket
