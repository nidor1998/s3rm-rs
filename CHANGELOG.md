# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v1.0.1] - 2026-02-26

### Added

- Crates.io version, crates.io downloads, and GitHub downloads badges to README
- E2E test for prefix boundary respect on versioned buckets
- E2E test for parallel `list_object_versions` with multi-level nested prefixes
- Unit tests for `S3Object::new` and `S3Object::new_versioned` constructors

## [v1.0.0] - 2026-02-25

Initial release.
