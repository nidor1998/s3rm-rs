# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v1.1.0] - 2026-02-28

### Added

- `--keep-latest-only` option for version retention policies. Retains only the latest version of each object in versioned buckets and deletes all older versions. Requires `--delete-all-versions`. Can be combined with `--filter-include-regex` and `--filter-exclude-regex` to target specific keys.

### Changed

- `--if-match` and `--delete-all-versions` are now mutually exclusive. S3 does not support If-Match conditional headers when deleting by version ID.
- **Library API**: `S3Object::is_latest()` now returns `true` for non-versioned objects and when the field is absent from the S3 response. Previously returned `false` in both cases, which could lead to unintended deletions.
- Display "Deletion cancelled." message when the user declines the confirmation prompt. Previously, no feedback was shown when entering anything other than "yes".

## [v1.0.2] - 2026-02-27

### Added

- Unit tests for pipeline panic and error handling (lister panic, filter error/panic, delete worker error/panic)
- Unit tests for user-defined callback registration in CLI binary
- Unit tests for indicator refresh-interval code paths (moving average, progress text)
- Unit tests for RecordingMockStorage and BatchRecordingMockStorage in optimistic locking
- Unit tests for VersioningMockStorage in versioning properties
- Unit tests for MockStorage list methods in lister
- Unit tests for pipeline mock storages (ListingMockStorage, PartialFailureMockStorage, FailingListerStorage, NonRetryableFailureMockStorage)

### Fixed

- `GetObjectTaggingOutput` builder panics in mock storages due to missing required `tag_set` field

## [v1.0.1] - 2026-02-26

### Added

- Crates.io version, crates.io downloads, and GitHub downloads badges to README
- E2E test for prefix boundary respect on versioned buckets
- E2E test for parallel `list_object_versions` with multi-level nested prefixes
- Unit tests for `S3Object::new` and `S3Object::new_versioned` constructors
- E2E test for parallel version listing pagination within sub-prefixes
- Unit tests for untested MockStorage trait methods in lister, deleter, and filters modules (22 tests)

## [v1.0.0] - 2026-02-25

Initial release.
