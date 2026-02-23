# Implementation Plan: s3rm-rs

## Executive Summary

**Project Status**: In progress

## Overview

This implementation plan follows a phased approach that maximizes code reuse from s3sync (~90% of codebase). The architecture is library-first, with the CLI as a thin wrapper. The implementation focuses on streaming pipelines with stages connected by async channels, targeting comprehensive property-based testing coverage for all critical correctness properties.

**Current Achievement**: Tasks 1-26 complete (including Tasks 24-25: comprehensive unit tests and property testing infrastructure). Task 23 checkpoint complete (CLI polish, spec audit, docs, infrastructure). Task 27 complete (27.1-27.5 all done). Task 28 complete (28.1 clippy, 28.2 rustfmt, 28.4 cargo-deny all pass). All 49 correctness properties have property-based tests (442 lib tests, 26 binary tests). Project setup, core infrastructure, core data models, storage layer, object lister, filter stages, Lua integration, deletion components, safety features, deletion pipeline, progress reporting, library API, CLI implementation, versioning support, retry/error handling, optimistic locking, logging/verbosity, AWS configuration, rate limiting, cross-platform support, CI/CD integration, additional property tests, comprehensive unit tests, and property testing infrastructure all established. Rustdoc documentation, examples, CONTRIBUTING.md, SECURITY.md, Dockerfile, build info (shadow-rs), and PR template added. Code quality checks (clippy, rustfmt, cargo-deny) all pass.

## Current Status

**Completed Phases**:
Phase 0: Project Setup (Task 1)
Phase 1: Core Infrastructure (Task 2)
Phase 2: Core Data Models (Task 3)
Phase 3: Storage Layer (Task 4)
Phase 4: Object Lister (Task 5)
Phase 5: Filter Stages (Task 6)
Phase 6: Lua Integration (Task 7)
Phase 7: Deletion Components (Task 8)
Phase 8: Safety Features (Task 9)
Phase 9: Deletion Pipeline (Task 10)
Phase 10: Progress Reporting (Task 11)
Phase 11: Library API (Task 12)
Phase 12: CLI Implementation (Task 13)
Phase 13: Versioning Support (Task 14)
Phase 14: Retry and Error Handling (Task 15)
Phase 15: Optimistic Locking Support (Task 16)
Phase 16: Logging and Verbosity (Task 17)
Phase 17: AWS Configuration Support (Task 18)
Phase 18: Rate Limiting (Task 19)
Phase 19: Cross-Platform Support (Task 20)
Phase 20: CI/CD Integration (Task 21)
Phase 21: Additional Property Tests (Task 22)
Phase 22: Verify All Property Tests (Task 26)
Phase 23: Checkpoint Review (Task 23)
Phase 24: Comprehensive Unit Tests (Task 24)
Phase 25: Property-Based Testing Infrastructure (Task 25)

## Tasks

- [x] 1. Project Setup and Foundation
  - Create Cargo workspace with s3rm-rs library and binary
  - Configure dependencies matching s3sync versions (AWS SDK 1.122.0, tokio 1.49, clap 4.5, mlua 0.11, proptest 1.6)
  - Set up project structure with lib.rs and main.rs
  - Configure CI pipeline for multi-platform builds (Linux x86_64/ARM64, Windows, macOS)
  - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5_


- [x] 2. Reuse Core Infrastructure from s3sync
  - [x] 2.1 Copy AWS client setup and configuration
    - Copy aws/client.rs with credential loading, region configuration, endpoint setup
    - Include zeroize for secure credential handling
    - Copy AwsCredentials struct with Zeroize and ZeroizeOnDrop derives
    - _Requirements: 8.4, 8.5, 8.6_

  - [x] 2.2 Copy retry policy and rate limiter
    - Copy aws/retry.rs with exponential backoff and error classification
    - Copy aws/rate_limiter.rs with token bucket algorithm
    - _Requirements: 6.1, 6.2, 6.6, 8.7_

  - [x] 2.3 Copy tracing infrastructure
    - Copy tracing/mod.rs with subscriber configuration
    - Support verbosity levels (quiet, normal, verbose, very verbose, debug)
    - Support JSON and text formats with color control
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9_

  - [x] 2.4 Copy configuration parsing infrastructure
    - Copy config/args.rs with parse_from_args function using clap
    - Copy config/mod.rs with Config struct and validation logic
    - Adapt to remove source-specific options (keep only target options)
    - Include zeroize for credential fields in Config
    - _Requirements: 8.1, 8.2, 8.3, 10.1, 10.2, 10.3, 10.4_

  - [x] 2.5 Copy cancellation token support
    - Copy types/token.rs with CancellationToken type alias
    - Implement create_pipeline_cancellation_token function
    - _Requirements: N/A (infrastructure)_


- [x] 3. Implement Core Data Models
  - [x] 3.1 Create S3Object and related types
    - Implement S3Object struct (key, version_id, size, last_modified, etag, storage_class, is_delete_marker)
    - Note: content_type, metadata, tags fetched on-demand in ObjectDeleter
    - Implement ObjectKeyMap type alias
    - _Requirements: 1.8, 5.3_

  - [x] 3.2 Create deletion statistics types
    - Implement DeletionStatistics struct (deleted_objects, deleted_bytes, failed_objects)
    - Implement DeletionStatsReport with atomic counters
    - Implement DeletionStats for public API
    - _Requirements: 6.5, 7.1, 7.3_

  - [x] 3.3 Create deletion outcome and error types
    - Implement DeletionOutcome enum (Success, Failed)
    - Implement DeletionError enum (NotFound, AccessDenied, PreconditionFailed, Throttled, NetworkError, ServiceError)
    - Implement Error enum with thiserror (AwsSdk, InvalidConfig, InvalidUri, InvalidRegex, LuaScript, Io, Cancelled, PartialFailure, Pipeline)
    - Include exit_code() and is_retryable() methods
    - _Requirements: 6.4, 6.5, 10.5, 13.4_

  - [x] 3.4 Create deletion event types
    - Implement DeletionEvent enum (PipelineStart, ObjectDeleted, ObjectFailed, PipelineEnd, PipelineError)
    - _Requirements: 7.6, 7.7_

  - [x] 3.5 Create S3Target type
    - Implement S3Target struct (bucket, prefix, endpoint, region)
    - Implement parse() method for s3:// URI parsing
    - _Requirements: 2.1, 8.5, 8.6_


- [x] 4. Implement Storage Layer (Reuse from s3sync)
  - [x] 4.1 Copy Storage trait and S3 storage implementation
    - Copy storage/mod.rs with Storage trait
    - Copy storage/s3.rs with S3 storage implementation
    - Adapt storage/factory.rs to create S3 storage only (no local storage needed)
    - Include is_versioning_enabled() method
    - _Requirements: 5.1, 5.2, 5.3_

  - [x] 4.2 Write unit tests for storage factory
    - Test S3 storage creation with various configurations
    - Test versioning detection
    - _Requirements: 5.1, 5.2, 5.3_


- [x] 5. Implement Object Lister (Reuse from s3sync)
  - [x] 5.1 Copy ObjectLister implementation
    - Copy lister/mod.rs with parallel pagination support
    - Support configurable Parallel_Lister_Count
    - Support Max_Parallel_Listing_Max_Depth option
    - Handle versioned bucket listing
    - _Requirements: 1.5, 1.6, 1.7, 5.3_

  - [x] 5.2 Write property test for parallel listing
    - **Property 5: Parallel Listing Configuration**
    - **Validates: Requirements 1.5, 1.6, 1.7**


- [x] 6. Implement Filter Stages (Reuse from s3sync)
  - [x] 6.1 Copy filter infrastructure
    - Copy pipeline/filter/mod.rs with ObjectFilter trait
    - Copy pipeline/stage.rs with Stage struct (adapted for deletion - only target storage)
    - _Requirements: 2.11_

  - [x] 6.2 Copy time filters
    - Copy pipeline/filter/mtime.rs with MtimeBeforeFilter and MtimeAfterFilter
    - _Requirements: 2.7_

  - [x] 6.3 Copy size filters
    - Copy pipeline/filter/size.rs with SmallerSizeFilter and LargerSizeFilter
    - _Requirements: 2.6_

  - [x] 6.4 Copy regex filters for keys
    - Copy pipeline/filter/regex.rs with IncludeRegexFilter and ExcludeRegexFilter
    - Note: Content-type, metadata, and tag filters are in ObjectDeleter (not separate stages)
    - _Requirements: 2.2_

  - [x] 6.5 Copy user-defined Lua filter
    - Copy pipeline/filter/user_defined.rs with UserDefinedFilter
    - _Requirements: 2.8_

  - [x] 6.6 Write property tests for filters
    - **Property 7: Prefix Filtering**
    - **Validates: Requirements 2.1**

  - [x] 6.7 Write property test for regex filtering
    - **Property 8: Regex Filtering**
    - **Validates: Requirements 2.2, 2.3, 2.4, 2.5, 2.12**

  - [x] 6.8 Write property test for size filtering
    - **Property 9: Size Range Filtering**
    - **Validates: Requirements 2.6**

  - [x] 6.9 Write property test for time filtering
    - **Property 10: Time Range Filtering**
    - **Validates: Requirements 2.7**


- [x] 7. Implement Lua Integration (Reuse from s3sync)
  - [x] 7.1 Copy Lua VM and callback infrastructure
    - Copy lua/vm.rs with LuaScriptCallbackEngine
    - Support safe mode (no OS/I/O), allow_lua_os_library mode, and unsafe mode
    - Implement memory limits
    - _Requirements: 2.13, 2.14_

  - [x] 7.2 Copy Lua filter callback
    - Copy lua/callbacks.rs with LuaFilterCallback
    - Implement FilterCallback trait
    - _Requirements: 2.8, 2.12_

  - [x] 7.3 Copy Lua event callback
    - Copy lua/callbacks.rs with LuaEventCallback
    - Implement EventCallback trait
    - _Requirements: 7.6, 7.7, 2.12_

  - [x] 7.4 Write property test for Lua filter callbacks
    - **Property 11: Lua Filter Callback Execution**
    - **Validates: Requirements 2.8**

  - [x] 7.5 Write property test for Lua callback types
    - **Property 14: Lua Callback Type Support**
    - **Validates: Requirements 2.12**

  - [x] 7.6 Write property test for Lua sandbox security
    - **Property 15: Lua Sandbox Security**
    - **Validates: Requirements 2.13, 2.14**


- [x] 8. Implement Deletion Components (New)
  - [x] 8.1 Implement BatchDeleter
    - Create pipeline/batch_deleter.rs
    - Implement Deleter trait with delete() method
    - Group objects into batches of up to 1000
    - Call S3 DeleteObjects API with version IDs when applicable
    - Parse response to identify successes and failures
    - Retry failed objects using retry policy
    - _Requirements: 1.1, 1.9, 5.5_

  - [x] 8.2 Implement SingleDeleter
    - Create pipeline/single_deleter.rs
    - Implement Deleter trait with delete() method
    - Delete objects one at a time using S3 DeleteObject API
    - Apply retry policy for each deletion
    - _Requirements: 1.2_

  - [x] 8.3 Implement ObjectDeleter worker
    - Create pipeline/deleter.rs
    - Adapt from s3sync's ObjectSyncer pattern
    - Read objects from MPMC channel (stage.receiver)
    - Apply content-type filters (HeadObject API call if configured)
    - Apply metadata filters (HeadObject API call if configured)
    - Apply tagging filters (GetObjectTagging API call if configured)
    - Delegate to BatchDeleter or SingleDeleter based on batch_size config
    - Update deletion statistics
    - Write results to stage.sender channel
    - _Requirements: 1.3, 2.3, 2.4, 2.5, 6.5_

  - [x] 8.4 Write property test for batch deletion API usage
    - **Property 1: Batch Deletion API Usage**
    - **Validates: Requirements 1.1, 5.5**

  - [x] 8.5 Write property test for single deletion API usage
    - **Property 2: Single Deletion API Usage**
    - **Validates: Requirements 1.2**

  - [x] 8.6 Write property test for concurrent worker execution
    - **Property 3: Concurrent Worker Execution**
    - **Validates: Requirements 1.3**

  - [x] 8.7 Write property test for partial batch failure recovery
    - **Property 6: Partial Batch Failure Recovery**
    - **Validates: Requirements 1.9**

  - [x] 8.8 Write unit tests for ObjectDeleter
    - Test content-type filtering with HeadObject
    - Test metadata filtering with HeadObject
    - Test tagging filtering with GetObjectTagging
    - Test filter combination (AND logic)
    - _Requirements: 2.3, 2.4, 2.5, 2.11_


- [x] 9. Implement Safety Features (New)
  - [x] 9.1 Implement SafetyChecker
    - Create safety/mod.rs with SafetyChecker struct
    - Implement check_before_deletion() method
    - Handle dry-run mode (skip confirmation — pipeline runs but deletion layer simulates)
    - Display target prefix with colored text in confirmation prompt
    - Implement confirmation prompt (require exact "yes" input)
    - Skip prompts if force flag is set
    - Skip prompts if JSON logging is enabled (would corrupt output)
    - Skip prompts in non-TTY environments
    - Use println/print for prompts (not tracing)
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 13.1_

  - [x] 9.2 Implement dry-run support in ObjectDeleter
    - Skip actual S3 API calls when dry_run is enabled
    - Simulate successful deletion for all objects in buffer
    - Log each object at info level with `[dry-run]` prefix (key, version_id, size)
    - Emit stats and events with dry_run flag set
    - _Requirements: 3.1_

  - [x] 9.3 Implement max-delete threshold enforcement in ObjectDeleter
    - Cancel pipeline when delete count exceeds --max-delete limit
    - Already implemented in ObjectDeleter.process_object() via delete_counter
    - _Requirements: 3.6_

  - [x] 9.4 Write property test for dry-run mode safety
    - **Property 16: Dry-Run Mode Safety**
    - **Validates: Requirements 3.1**

  - [x] 9.5 Write property test for confirmation prompt validation
    - **Property 17: Confirmation Prompt Validation**
    - **Validates: Requirements 3.3**

  - [x] 9.6 Write property test for force flag behavior
    - **Property 18: Force Flag Behavior**
    - **Validates: Requirements 3.4, 13.2**

  - [x] 9.7 Write unit tests for dry-run in ObjectDeleter
    - Test dry-run skips API calls and reports correct stats
    - Test dry-run works in single mode (batch_size=1)
    - Test dry-run preserves version IDs for versioned objects
    - _Requirements: 3.1_

  - [x] 9.8 Write unit tests for per-object info logging
    - Test info-level logging with key, version_id, size for each deleted object
    - Test `[dry-run]` prefix in dry-run mode
    - Test normal mode logs without `[dry-run]` prefix
    - _Requirements: 3.1, 3.5_


- [x] 10. Implement Deletion Pipeline (Adapted from s3sync)
  - [x] 10.1 Create DeletionPipeline struct
    - Create pipeline/mod.rs with DeletionPipeline
    - Include config, target storage, cancellation_token, stats_receiver, error tracking
    - Adapt from s3sync's Pipeline structure
    - _Requirements: 1.8_

  - [x] 10.2 Implement pipeline initialization
    - Implement new() method
    - Initialize storage using factory (S3 only)
    - Create stats channel
    - Initialize error tracking with Arc<AtomicBool> and Arc<Mutex<VecDeque<Error>>>
    - _Requirements: 1.8_

  - [x] 10.3 Implement pipeline stages
    - Implement list_target() - spawn ObjectLister
    - Implement filter_objects() - chain filter stages
    - Implement delete_objects() - spawn ObjectDeleter workers (MPMC)
    - Implement terminate() - spawn Terminator
    - Connect stages with async channels (bounded)
    - _Requirements: 1.3, 1.5, 2.11_

  - [x] 10.4 Implement pipeline run() method
    - Check prerequisites (versioning, dry-run, confirmation)
    - Create and connect all stages
    - Wait for completion
    - Handle cancellation via cancellation_token
    - _Requirements: 3.1, 3.2, 5.1_

  - [x] 10.5 Implement error and statistics methods
    - Implement has_error(), get_errors_and_consume(), has_warning()
    - Implement get_deletion_stats()
    - Implement close_stats_sender()
    - _Requirements: 6.4, 6.5, 7.3_

  - [x] 10.6 Copy Terminator implementation
    - Copy pipeline/terminator.rs from s3sync
    - Consume final stage output and close channels
    - _Requirements: N/A (infrastructure)_

  - [x] 10.7 Write unit tests for pipeline stages
    - Test stage creation and connection
    - Test channel communication between stages
    - Test cancellation handling
    - _Requirements: 1.3, 1.8_


- [x] 11. Implement Progress Reporting (Reuse from s3sync)
  - [x] 11.1 Copy progress reporter
    - Copy progress/mod.rs with show_indicator() function
    - Use indicatif for progress bar
    - Calculate moving averages for throughput
    - Display final summary with statistics
    - Support quiet mode
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5_

  - [x] 11.2 Write property test for progress reporting
    - **Property 31: Progress Reporting**
    - **Validates: Requirements 7.1, 7.3, 7.4**

  - [x] 11.3 Write property test for event callback invocation
    - **Property 32: Event Callback Invocation**
    - **Validates: Requirements 7.6, 7.7**


- [x] 12. Implement Library API (Public Interface)
  - [x] 12.1 Create lib.rs with public API
    - Export DeletionPipeline, Config, ParsedArgs
    - Export parse_from_args() function
    - Export create_pipeline_cancellation_token() function
    - Export DeletionStats struct
    - Export FilterCallback and EventCallback traits
    - Follow s3sync's API pattern exactly
    - Note: lib.rs already exports all modules publicly; this task adds re-exports of key types at root level once DeletionPipeline and ParsedArgs exist
    - _Requirements: 12.1, 12.2, 12.3, 12.4_

  - [x] 12.2 Implement callback traits for Rust API
    - Define FilterCallback trait with filter() method — done in types/filter_callback.rs
    - Define EventCallback trait with on_event() method — done in types/event_callback.rs
    - Support registration via Config — done via FilterManager and EventManager in callback/
    - _Requirements: 12.5, 12.6_

  - [x] 12.3 Add rustdoc documentation
    - Document all public types and functions
    - Include usage examples in doc comments
    - Document library-first architecture
    - _Requirements: 12.9_

  - [x] 12.4 Write property test for library API configuration
    - **Property 44: Library API Configuration**
    - **Validates: Requirements 12.4**

  - [x] 12.5 Write property test for library callback registration
    - **Property 45: Library Callback Registration**
    - **Validates: Requirements 12.5, 12.6**

  - [x] 12.6 Write property test for library Lua callback support
    - **Property 46: Library Lua Callback Support**
    - **Validates: Requirements 12.7**

  - [x] 12.7 Write property test for library async result handling
    - **Property 47: Library Async Result Handling**
    - **Validates: Requirements 12.8**


- [x] 13. Implement CLI (Adapted from s3sync)
  - [x] 13.1 Define CLI arguments with clap
    - Create config/args.rs with CliArgs struct
    - Define target argument (s3://bucket/prefix)
    - Define deletion options (batch_size, delete_all_versions)
    - Define safety options (dry_run, force, max_delete)
    - Define filter options (same as s3sync: regex, size, time, Lua)
    - Define performance options (same as s3sync: worker_size, parallel_listings)
    - Define logging options (same as s3sync: verbosity, json, color)
    - Define retry options (same as s3sync)
    - Define timeout options (same as s3sync)
    - Define AWS config options (target-* only, no source-*)
    - Define Lua options (same as s3sync)
    - Define advanced options (if_match, warn_as_error, max_keys, show_no_progress)
    - Support environment variables for all options
    - _Requirements: 8.1, 8.2, 10.1, 10.2, 10.3, 10.4, 10.7_

  - [x] 13.2 Implement parse_from_args function
    - Parse CLI arguments using clap
    - Handle environment variables
    - Apply configuration precedence (CLI > env > defaults)
    - Return ParsedArgs struct
    - Reuse s3sync's parsing pattern
    - _Requirements: 8.1, 8.2, 10.2_

  - [x] 13.3 Implement Config::try_from(ParsedArgs)
    - Validate all arguments
    - Build Config struct
    - Apply defaults
    - Handle batch_size special case (Express One Zone defaults to 1)
    - Reuse s3sync's validation logic
    - _Requirements: 8.3, 10.2_

  - [x] 13.4 Implement main.rs CLI entry point
    - Collect CLI args from std::env::args()
    - Call parse_from_args() and Config::try_from()
    - Initialize tracing with configured verbosity
    - Create cancellation token
    - Setup Ctrl-C handler
    - Create and run DeletionPipeline
    - Handle errors and display statistics
    - Return appropriate exit codes
    - Follow s3sync's main.rs pattern exactly
    - _Requirements: 10.5, 13.4, 13.6_

  - [x] 13.5 Write property test for configuration precedence
    - **Property 33: Configuration Precedence**
    - **Validates: Requirements 8.1, 8.2, 8.3, 8.5**

  - [x] 13.6 Write property test for input validation
    - **Property 38: Input Validation and Error Messages**
    - **Validates: Requirements 10.2**

  - [x] 13.7 Write property test for flag alias support
    - **Property 39: Flag Alias Support**
    - **Validates: Requirements 10.4**

  - [x] 13.8 Write property test for exit code mapping
    - **Property 40: Exit Code Mapping**
    - **Validates: Requirements 10.5, 13.4**


- [x] 14. Implement Versioning Support
  - [x] 14.1 Add version handling to ObjectDeleter
    - S3Object enum supports NotVersioning, Versioning, and DeleteMarker variants (Task 3)
    - BatchDeleter and SingleDeleter include version_id in API requests (Task 8)
    - Config has delete_all_versions flag; ObjectLister dispatches versioned listing (Task 5)
    - _Requirements: 5.1, 5.2_

  - [x] 14.2 ~~Add version display to dry-run mode~~ Not needed: each version is a separate object in the pipeline, so progress/summary counts are already correct
    - _Requirements: 5.4_

  - [x] 14.3 Write property test for versioned bucket delete marker creation
    - **Property 25: Versioned Bucket Delete Marker Creation**
    - **Validates: Requirements 5.1**

  - [x] 14.4 Write property test for all-versions deletion
    - **Property 26: All-Versions Deletion**
    - **Validates: Requirements 5.2**

  - [x] 14.5 Write property test for version information retrieval
    - **Property 27: Version Information Retrieval**
    - **Validates: Requirements 5.3**

  - [x] 14.6 Write property test for versioned dry-run display
    - **Property 28: Versioned Dry-Run Display**
    - **Validates: Requirements 5.4**


- [x] 15. Implement Retry and Error Handling
  - [x] 15.1 Verify retry policy integration
    - AWS SDK retry with exponential backoff configured in client_builder.rs (Task 2)
    - Force retry loop in S3Storage::delete_objects() for partial batch failures
    - DeletionError::is_retryable() classifies Throttled, NetworkError, ServiceError as retryable
    - _Requirements: 6.1, 6.2, 6.6_

  - [x] 15.2 Implement failure tracking
    - DeletionStatsReport tracks failed_objects with atomic counters (Task 3)
    - ObjectDeleter logs failures and continues processing remaining objects (Task 8)
    - _Requirements: 6.4, 6.5_

  - [x] 15.3 Write property test for retry with exponential backoff
    - **Property 29: Retry with Exponential Backoff**
    - **Validates: Requirements 6.1, 6.2, 6.6**

  - [x] 15.4 Write property test for failure tracking and continuation
    - **Property 30: Failure Tracking and Continuation**
    - **Validates: Requirements 6.4, 6.5**

  - [x] 15.5 Write unit tests for error handling
    - DeletionError::is_retryable() tested — 16 unit tests in types/error.rs (Task 3)
    - S3rmError::exit_code() tested for all variants (Task 3)
    - Partial failure scenarios tested in deleter/tests.rs (Task 8)
    - _Requirements: 6.1, 6.2, 6.4_


- [x] 16. Implement Optimistic Locking Support
  - [x] 16.1 Add If-Match support to deletion requests
    - if_match field in Config (Task 2)
    - BatchDeleter includes ETag when if_match enabled (Task 8)
    - SingleDeleter includes ETag when if_match enabled (Task 8)
    - DeletionError::PreconditionFailed variant handles 412 responses (Task 3)
    - Unit tests cover both enabled/disabled if_match paths
    - _Requirements: 11.1, 11.2, 11.3_

  - [x] 16.2 Handle conditional deletion failures
    - PreconditionFailed errors logged and tracked in failed count
    - Objects with failed conditions are skipped
    - _Requirements: 11.2_

  - [x] 16.3 Write property test for If-Match conditional deletion
    - **Property 41: If-Match Conditional Deletion**
    - **Validates: Requirements 11.1, 11.2**
    - Implemented in src/optimistic_locking_properties.rs (Task 16)

  - [x] 16.4 Write property test for If-Match flag propagation
    - **Property 42: If-Match Flag Propagation**
    - **Validates: Requirements 11.3**
    - Implemented in src/optimistic_locking_properties.rs (Task 16)

  - [x] 16.5 Write property test for batch conditional deletion handling
    - **Property 43: Batch Conditional Deletion Handling**
    - **Validates: Requirements 11.4**
    - Implemented in src/optimistic_locking_properties.rs (Task 16)


- [x] 17. Implement Logging and Verbosity
  - [x] 17.1 Verify tracing integration
    - init_tracing() in bin/s3rm/tracing_init.rs supports all verbosity levels, JSON, text, color (Task 2)
    - TracingConfig in Config covers tracing_level, json_tracing, aws_sdk_tracing, disable_color_tracing
    - All components use tracing macros (info!, warn!, error!, debug!, trace!)
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9_

  - [x] 17.2 Implement error logging
    - ObjectDeleter logs deletion failures with key, version_id, error context (Task 8)
    - Per-object info logging with [dry-run] prefix in dry-run mode (Task 9)
    - _Requirements: 4.10_

  - [x] 17.3 Write property test for verbosity level configuration
    - **Property 21: Verbosity Level Configuration**
    - **Validates: Requirements 4.1**
    - Implemented in src/logging_properties.rs (Task 17)

  - [x] 17.4 Write property test for JSON logging format
    - **Property 22: JSON Logging Format**
    - **Validates: Requirements 4.7, 13.3**
    - Implemented in src/logging_properties.rs (Task 17)

  - [x] 17.5 Write property test for color output control
    - **Property 23: Color Output Control**
    - **Validates: Requirements 4.8, 4.9, 7.5, 13.7**
    - Implemented in src/logging_properties.rs (Task 17)

  - [x] 17.6 Write property test for error logging
    - **Property 24: Error Logging**
    - **Validates: Requirements 4.10**
    - Implemented in src/logging_properties.rs (Task 17)


- [x] 18. Implement AWS Configuration Support
  - [x] 18.1 Verify AWS credential loading
    - client_builder.rs supports access keys, named profiles, and environment credentials (Task 2)
    - Region configuration from profile, env, or explicit setting (Task 2)
    - _Requirements: 8.4, 13.5_

  - [x] 18.2 Verify custom endpoint support
    - endpoint_url in ClientConfig, applied in client_builder.rs (Task 2)
    - force_path_style for S3-compatible services (MinIO, Wasabi, LocalStack)
    - _Requirements: 8.6_

  - [x] 18.3 Write property test for AWS credential loading
    - **Property 34: AWS Credential Loading**
    - **Validates: Requirements 8.4**
    - Implemented in src/aws_config_properties.rs (Task 18)

  - [x] 18.4 Write property test for custom endpoint support
    - **Property 35: Custom Endpoint Support**
    - **Validates: Requirements 8.6**
    - Implemented in src/aws_config_properties.rs (Task 18)


- [x] 19. Implement Rate Limiting
  - [x] 19.1 Verify rate limiter integration
    - leaky_bucket RateLimiter integrated in storage layer (Task 2)
    - Token bucket applied to all S3 operations: list, head, delete, get-tagging
    - rate_limit_objects in Config; unit tests for rate limiter creation
    - _Requirements: 8.7_

  - [x] 19.2 Write property test for rate limiting enforcement
    - **Property 36: Rate Limiting Enforcement**
    - **Validates: Requirements 8.7**
    - Implemented in src/rate_limiting_properties.rs (Task 19)


- [x] 20. Implement Cross-Platform Support
  - [x] 20.1 Verify path handling
    - Test file path normalization on Windows
    - Test file path normalization on Unix
    - Ensure Lua script paths work cross-platform
    - Implemented in src/cross_platform_properties.rs and src/config/args/value_parser/file_exist.rs (Task 20)
    - _Requirements: 9.6_

  - [x] 20.2 Verify terminal feature detection
    - std::io::IsTerminal used in safety module for TTY detection (Task 9)
    - Color control via TracingConfig::disable_color_tracing (Task 2)
    - _Requirements: 9.7_

  - [x] 20.3 Configure cross-platform builds
    - CI configured in .github/workflows/ci.yml (Task 1)
    - Linux x86_64 glibc/musl, ARM64 glibc/musl, Windows x86_64/aarch64, macOS x86_64/aarch64
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.8_

  - [x] 20.4 Write property test for cross-platform path handling
    - **Property 37: Cross-Platform Path Handling**
    - **Validates: Requirements 9.6**
    - Implemented in src/cross_platform_properties.rs (Task 20)


- [x] 21. Implement CI/CD Integration Features
  - [x] 21.1 Implement non-interactive environment detection
    - SafetyChecker uses std::io::IsTerminal (not atty) for TTY detection (Task 9)
    - Prompts skipped in non-TTY environments and when JSON logging is enabled
    - _Requirements: 13.1_

  - [x] 21.2 Verify output stream separation
    - tracing-subscriber::fmt() writes all logs (including errors) to stdout by default (Tasks 2, 13)
    - This is the intended behavior per Requirement 13.6
    - _Requirements: 13.6_

  - [x] 21.3 Write property test for non-interactive environment detection
    - **Property 48: Non-Interactive Environment Detection**
    - **Validates: Requirements 13.1**
    - Implemented in src/cicd_properties.rs (Task 21)

  - [x] 21.4 Write property test for output stream separation
    - **Property 49: Output Stream Separation**
    - **Validates: Requirements 13.6**
    - Implemented in src/cicd_properties.rs (Task 21)


- [x] 22. Implement Additional Property Tests
  - [x] 22.1 Write property test for worker count configuration validation
    - **Property 4: Worker Count Configuration Validation**
    - **Validates: Requirements 1.4**

  - [x] 22.2 Write property test for Rust filter callback execution
    - **Property 12: Rust Filter Callback Execution**
    - **Validates: Requirements 2.9**

  - [x] 22.3 Write property test for delete-all behavior
    - **Property 13: Delete-All Behavior**
    - **Validates: Requirements 2.10**


- [x] 23. Checkpoint - Review Current Implementation
  - [x] Review all completed implementation tasks
  - [x] Verify all core deletion functionality is working
  - [x] Verify all filter stages are connected correctly
  - [x] Verify safety features are working (dry-run, confirmation)
  - [x] Run existing property tests and verify they pass
  - [x] Identify any gaps or issues
  - [x] CLI polish: unified command examples in README, content-type filter help, --json-tracing requires -f help, --batch-size moved to Performance, dry-run hint in confirmation prompt, last_modified in deletion logs, rate-limit-objects vs batch-size validation
  - [x] Infrastructure: build info (shadow-rs), Dockerfile, SECURITY.md, PR template, CONTRIBUTING.md AI notice
  - [x] Spec audit: updated requirements.md, design.md, structure.md to match implementation
  - [x] Document exit code 101 for abnormal termination


- [x] 24. Write Comprehensive Unit Tests
  - [x] 24.1 Write unit tests for configuration parsing
    - 54 unit tests in config/args/tests.rs covering argument parsing, environment variables, validation, and invalid input (Task 13)
    - 4 unit tests in config/args/value_parser/human_bytes.rs for human-readable byte parsing (Task 13)
    - _Requirements: 8.1, 8.2, 8.3, 10.2_

  - [x] 24.2 Write unit tests for filter logic
    - Property tests cover regex, size, time filters (Tasks 6)
    - Filter combination (AND logic) tested in ObjectDeleter tests (Task 8)
    - _Requirements: 2.2, 2.6, 2.7, 2.11_

  - [x] 24.3 Write unit tests for deletion logic with mocks
    - 30+ tokio tests in deleter/tests.rs cover batch grouping, single deletion, partial failures (Task 8)
    - MockStorage used for all deletion tests
    - if_match and version_id paths tested
    - _Requirements: 1.1, 1.2, 1.9, 6.1, 6.2_

  - [x] 24.4 Write unit tests for safety features
    - SafetyChecker tests for dry-run, force flag, non-TTY, JSON logging skip (Task 9)
    - ObjectDeleter dry-run tests verify no API calls and correct stats (Task 9)
    - Max-delete threshold tested (Task 9)
    - _Requirements: 3.1, 3.3, 3.4, 3.6_

  - [x] 24.5 Write unit tests for versioning support
    - Version ID handling tested in batch and single deleter tests (Task 8)
    - S3Object variants (NotVersioning, Versioning, DeleteMarker) tested (Task 3)
    - Dry-run with versioned objects tested (Task 9)
    - _Requirements: 5.1, 5.2, 5.5_

  - [x] 24.6 Write unit tests for error handling
    - DeletionError::is_retryable() tested (Task 3)
    - S3rmError::exit_code() tested (Task 3)
    - Partial failure scenarios tested in deleter tests (Task 8)
    - _Requirements: 6.1, 6.4, 10.5, 13.4_


- [x] 25. Set Up Property-Based Testing Infrastructure
  - [x] 25.1 Review existing property test generators
    - proptest generators exist in filters/filter_properties.rs, deleter/tests.rs, lua/lua_properties.rs, safety/safety_properties.rs, lister.rs
    - S3Object generators, config generators, datetime generators all present
    - proptest configured with appropriate iteration counts
    - _Requirements: N/A (testing infrastructure)_

  - [x] 25.2 Create additional test utilities if needed
    - MockStorage implemented in deleter/tests.rs and reused across test files
    - Test fixtures and helpers present in multiple test modules
    - _Requirements: N/A (testing infrastructure)_


- [x] 26. Verify All Property Tests Are Implemented
  - [x] Review all 49 correctness properties from design document — all 49 confirmed present
  - [x] Verify each property has a corresponding property-based test file — 15 property test files with 83 proptest functions
  - [x] Verify each test runs with appropriate iteration counts
  - [x] Verify each test includes comment tag: "Feature: s3rm-rs, Property N: [property text]"
  - [x] Run all property tests: `cargo test --lib` — 442 tests pass
  - [x] Document any missing property tests — none missing
  - _Requirements: All requirements (comprehensive coverage)_


**Implemented Property Tests**: Properties 1-49 (49 of 49). All correctness properties have property-based tests. Test totals: 442 lib tests (all pass), 26 binary tests (all pass).

- [x] 27. Documentation and Examples
  - [x] 27.1 Write README.md (completed in commit 4082dbe on build/init/task23)
    - Include project overview and features
    - Include installation instructions
    - Include usage examples (CLI and library)
    - Include CLI reference with common options
    - Include library usage examples
    - Include links to documentation
    - _Requirements: 10.1, 10.7, 12.9_

  - [x] 27.2 Write rustdoc documentation (completed in commit c9bedf8 on build/init/task23)
    - Document all public API types and functions in lib.rs
    - Include code examples in doc comments
    - Document library-first architecture
    - Document callback registration (Rust and Lua)
    - Document configuration options
    - Run `cargo doc --open` to verify
    - _Requirements: 12.9_

  - [x] 27.3 Create example scripts (completed in commit c9bedf8 on build/init/task23)
    - Create example Lua filter script (examples/filter.lua)
    - Create example Lua event callback script (examples/event.lua)
    - Create example Rust library usage (examples/library_usage.rs)
    - Document each example with comments
    - _Requirements: 2.8, 7.6, 12.1_

  - [x] 27.4 Write CHANGELOG.md (completed in commit de7157d on build/init/task23)
    - Document initial release features
    - Document differences from s3sync
    - Document breaking changes (if any)
    - _Requirements: N/A (documentation)_

  - [x] 27.5 Write CONTRIBUTING.md (completed in commit c9bedf8 on build/init/task23)
    - Document development setup
    - Document testing requirements
    - Document code style guidelines
    - Document PR process
    - _Requirements: N/A (documentation)_


- [x] 28. Code Quality and Coverage
  - [x] 28.1 Run clippy and fix all warnings
    - `cargo clippy --all-targets --all-features` passes with zero warnings
    - CI enforces clippy in .github/workflows/ci.yml
    - _Requirements: N/A (code quality)_

  - [x] 28.2 Run rustfmt and format all code
    - `cargo fmt -- --check` passes (all code formatted)
    - CI enforces formatting in .github/workflows/ci.yml
    - _Requirements: N/A (code quality)_

  - [x] 28.4 Run security audit
    - cargo-deny configured in deny.toml (added in Task 23)
    - `cargo deny check` passes: advisories ok, bans ok, licenses ok, sources ok
    - CI enforces cargo deny in .github/workflows/ci.yml
    - _Requirements: N/A (security)_



- [ ] 29. Automated E2E Integration Testing

  ### E2E Test Requirements Checklist

  - [ ] Tests use `#![cfg(e2e_test)]` attribute (compiled only with `RUSTFLAGS="--cfg e2e_test"`)
  - [ ] All tests use the AWS profile `s3rm-e2e-test` (via `--target-profile s3rm-e2e-test`)
  - [ ] All E2E tests are executable in parallel (no shared state between tests; each test has a unique bucket)
  - [ ] Tests invoke the s3rm-rs library API (`build_config_from_args` + `DeletionPipeline`) -- not the CLI binary
  - [ ] Maximize source code coverage across all test cases
  - [ ] Test both Lua callbacks and Rust callbacks
  - [ ] No network disconnection/failure tests, BUT access denial tests ARE required
  - [ ] Bucket names are randomly generated per test case (use UUID v4 prefix: `s3rm-e2e-{uuid}`)
  - [ ] Post-processing ALWAYS runs regardless of test success/failure: delete all objects + delete bucket
  - [ ] Test data is auto-generated by the tests and uploaded to the target bucket (few KB per object, sufficient count)
  - [ ] Tests may use AWS SDK directly for test preparation (creating buckets, uploading objects, enabling versioning)
  - [ ] No performance testing in E2E tests
  - [ ] All CLI options tested; filtering options individually (one test per filter type); other options grouped logically
  - [ ] Maximum 1000 objects per E2E test case
  - [ ] Each test function name must be descriptive and clearly convey what is being tested
  - [ ] Each test function MUST begin with a detailed comment block explaining: (1) the purpose of the test, (2) the test setup, (3) the expected results. This is required for human review.

  ### E2E Test Infrastructure

  - [ ] 29.0 Create shared test helper module (`tests/common/mod.rs`)
    - `TestHelper` struct wrapping an S3 `Client` built with profile `s3rm-e2e-test`
    - `create_bucket(bucket)` -- creates a standard S3 bucket
    - `create_versioned_bucket(bucket)` -- creates bucket + enables versioning
    - `delete_bucket_cascade(bucket)` -- deletes all objects/versions + deletes bucket (always runs in cleanup)
    - `put_object(bucket, key, body)` -- uploads an object with given body bytes
    - `put_object_with_content_type(bucket, key, body, content_type)` -- uploads with explicit content type
    - `put_object_with_metadata(bucket, key, body, metadata: HashMap)` -- uploads with user-defined metadata
    - `put_object_with_tags(bucket, key, body, tags: HashMap)` -- uploads with tagging
    - `list_objects(bucket, prefix) -> Vec<String>` -- lists remaining object keys (for assertions)
    - `list_object_versions(bucket) -> Vec<(String, String)>` -- lists version IDs (for versioning assertions)
    - `count_objects(bucket, prefix) -> usize` -- counts objects remaining after deletion
    - `generate_bucket_name() -> String` -- returns `s3rm-e2e-{uuid}` for test isolation
    - `build_config(args: Vec<&str>) -> Config` -- calls `build_config_from_args` with common defaults prepended
    - `run_pipeline(config: Config) -> PipelineResult` -- creates token, runs pipeline, returns stats + error state
    - `init_tracing()` -- initializes a dummy tracing subscriber for test output
    - `PipelineResult` struct: `stats: DeletionStats`, `has_error: bool`, `has_warning: bool`, `errors: Vec<String>`
    - Region constant: use region from the `s3rm-e2e-test` profile (e.g., `us-east-1`)
    - All helper methods are `async` and use `tokio`

  ### E2E Test Plan: Filtering Tests (one test per filter type)

  Each filtering test uploads ~20 objects with varying properties, runs the pipeline with the specified filter, and asserts that only matching objects were deleted while non-matching objects remain.

  - [ ] 29.1 `e2e_filter_include_regex`
    - **Tests**: `--filter-include-regex`
    - **Setup**: Upload 20 objects: 10 with keys matching `^logs/.*\.txt$`, 10 with keys like `data/file.csv`
    - **Config**: `--filter-include-regex "^logs/.*\\.txt$" --force`
    - **Assertions**: Only the 10 `logs/*.txt` objects deleted; 10 `data/` objects remain
    - _Requirements: 2.2_

  - [ ] 29.2 `e2e_filter_exclude_regex`
    - **Tests**: `--filter-exclude-regex`
    - **Setup**: Upload 20 objects: 10 with keys matching `^keep/`, 10 with keys like `delete/file.dat`
    - **Config**: `--filter-exclude-regex "^keep/" --force`
    - **Assertions**: 10 `delete/` objects deleted; 10 `keep/` objects remain
    - _Requirements: 2.2_

  - [ ] 29.3 `e2e_filter_include_content_type_regex`
    - **Tests**: `--filter-include-content-type-regex`
    - **Setup**: Upload 10 objects with `Content-Type: text/plain`, 10 with `Content-Type: application/json`
    - **Config**: `--filter-include-content-type-regex "text/plain" --force`
    - **Assertions**: 10 text/plain objects deleted; 10 application/json objects remain
    - _Requirements: 2.3_

  - [ ] 29.4 `e2e_filter_exclude_content_type_regex`
    - **Tests**: `--filter-exclude-content-type-regex`
    - **Setup**: Upload 10 objects with `Content-Type: image/png`, 10 with `Content-Type: text/html`
    - **Config**: `--filter-exclude-content-type-regex "image/png" --force`
    - **Assertions**: 10 text/html objects deleted; 10 image/png objects remain
    - _Requirements: 2.3_

  - [ ] 29.5 `e2e_filter_include_metadata_regex`
    - **Tests**: `--filter-include-metadata-regex`
    - **Setup**: Upload 10 objects with metadata `env=production`, 10 with metadata `env=staging`
    - **Config**: `--filter-include-metadata-regex "env=production" --force`
    - **Assertions**: 10 `env=production` objects deleted; 10 `env=staging` objects remain
    - _Requirements: 2.4_

  - [ ] 29.6 `e2e_filter_exclude_metadata_regex`
    - **Tests**: `--filter-exclude-metadata-regex`
    - **Setup**: Upload 10 objects with metadata `tier=archive`, 10 with metadata `tier=hot`
    - **Config**: `--filter-exclude-metadata-regex "tier=archive" --force`
    - **Assertions**: 10 `tier=hot` objects deleted; 10 `tier=archive` objects remain
    - _Requirements: 2.4_

  - [ ] 29.7 `e2e_filter_include_tag_regex`
    - **Tests**: `--filter-include-tag-regex`
    - **Setup**: Upload 10 objects tagged `status=expired`, 10 tagged `status=active`
    - **Config**: `--filter-include-tag-regex "status=expired" --force`
    - **Assertions**: 10 `status=expired` objects deleted; 10 `status=active` objects remain
    - _Requirements: 2.5_

  - [ ] 29.8 `e2e_filter_exclude_tag_regex`
    - **Tests**: `--filter-exclude-tag-regex`
    - **Setup**: Upload 10 objects tagged `retain=true`, 10 tagged `retain=false`
    - **Config**: `--filter-exclude-tag-regex "retain=true" --force`
    - **Assertions**: 10 `retain=false` objects deleted; 10 `retain=true` objects remain
    - _Requirements: 2.5_

  - [ ] 29.9 `e2e_filter_mtime_before`
    - **Tests**: `--filter-mtime-before`
    - **Setup**: Upload 10 objects now. Then upload 10 more objects. Record a timestamp `T` between the two batches.
    - **Config**: `--filter-mtime-before <T in RFC 3339> --force`
    - **Assertions**: Only the first 10 (older) objects deleted; newer 10 remain
    - **Note**: Need a sleep or known timestamp between batches to reliably separate them
    - _Requirements: 2.7_

  - [ ] 29.10 `e2e_filter_mtime_after`
    - **Tests**: `--filter-mtime-after`
    - **Setup**: Same as 29.9 with timestamp `T` between two batches
    - **Config**: `--filter-mtime-after <T in RFC 3339> --force`
    - **Assertions**: Only the second 10 (newer) objects deleted; older 10 remain
    - _Requirements: 2.7_

  - [ ] 29.11 `e2e_filter_smaller_size`
    - **Tests**: `--filter-smaller-size`
    - **Setup**: Upload 10 objects of 100 bytes each, 10 objects of 10KB each
    - **Config**: `--filter-smaller-size 1KB --force`
    - **Assertions**: 10 small (100B) objects deleted; 10 large (10KB) objects remain
    - _Requirements: 2.6_

  - [ ] 29.12 `e2e_filter_larger_size`
    - **Tests**: `--filter-larger-size`
    - **Setup**: Upload 10 objects of 100 bytes each, 10 objects of 10KB each
    - **Config**: `--filter-larger-size 1KB --force`
    - **Assertions**: 10 large (10KB) objects deleted; 10 small (100B) objects remain
    - _Requirements: 2.6_

  ### E2E Test Plan: Core Deletion Mode Tests

  - [ ] 29.13 `e2e_basic_prefix_deletion`
    - **Tests**: Basic deletion by prefix (positional `target` argument)
    - **Setup**: Upload 30 objects under prefix `data/`, 10 objects under prefix `other/`
    - **Config**: `s3://{bucket}/data/ --force`
    - **Assertions**: All 30 `data/` objects deleted; 10 `other/` objects remain; stats show 30 deleted, 0 failed
    - _Requirements: 1.1, 2.1_

  - [ ] 29.14 `e2e_batch_deletion_mode`
    - **Tests**: Batch deletion with default batch size (200), `--batch-size` option
    - **Setup**: Upload 500 objects under a single prefix
    - **Config**: `--batch-size 100 --force`
    - **Assertions**: All 500 objects deleted; stats show 500 deleted, 0 failed
    - _Requirements: 1.1, 1.9_

  - [ ] 29.15 `e2e_single_deletion_mode`
    - **Tests**: Single-object deletion mode (`--batch-size 1`)
    - **Setup**: Upload 20 objects
    - **Config**: `--batch-size 1 --force`
    - **Assertions**: All 20 objects deleted; stats show 20 deleted, 0 failed
    - _Requirements: 1.2_

  - [ ] 29.16 `e2e_delete_entire_bucket_contents`
    - **Tests**: Deleting all objects in a bucket (no prefix)
    - **Setup**: Upload 50 objects with varied prefixes (`a/`, `b/`, `c/`, root-level)
    - **Config**: `s3://{bucket} --force` (no prefix -- deletes everything)
    - **Assertions**: All 50 objects deleted; bucket is empty; stats show 50 deleted
    - _Requirements: 2.10_

  - [ ] 29.17 `e2e_empty_bucket_no_error`
    - **Tests**: Running against an empty bucket should complete without error
    - **Setup**: Create empty bucket (no objects)
    - **Config**: `s3://{bucket} --force`
    - **Assertions**: Pipeline completes without error; stats show 0 deleted, 0 failed
    - _Requirements: 1.8_

  ### E2E Test Plan: Safety Feature Tests

  - [ ] 29.18 `e2e_dry_run_no_deletion`
    - **Tests**: `--dry-run` flag prevents actual deletions
    - **Setup**: Upload 20 objects
    - **Config**: `--dry-run --force`
    - **Assertions**: All 20 objects still exist after pipeline; stats show 20 "deleted" (simulated); no S3 deletions occurred
    - _Requirements: 3.1_

  - [ ] 29.19 `e2e_max_delete_threshold`
    - **Tests**: `--max-delete` stops deletion after threshold
    - **Setup**: Upload 50 objects
    - **Config**: `--max-delete 10 --force`
    - **Assertions**: Exactly 10 objects deleted; 40 remain; pipeline reports cancellation or partial completion
    - _Requirements: 3.6_

  - [ ] 29.20 `e2e_force_flag_skips_confirmation`
    - **Tests**: `--force` flag (pipeline runs without prompt in library API; this test confirms force=true works)
    - **Setup**: Upload 10 objects
    - **Config**: `--force`
    - **Assertions**: All 10 objects deleted; no prompt interaction required
    - _Requirements: 3.4, 13.2_

  ### E2E Test Plan: Versioning Tests

  - [ ] 29.21 `e2e_versioned_bucket_creates_delete_markers`
    - **Tests**: Deleting from versioned bucket without `--delete-all-versions` creates delete markers
    - **Setup**: Create versioned bucket; upload 10 objects (creates initial versions)
    - **Config**: `--force` (no `--delete-all-versions`)
    - **Assertions**: Delete markers created; original versions still exist; `list_object_versions` shows both markers and original versions
    - _Requirements: 5.1_

  - [ ] 29.22 `e2e_delete_all_versions`
    - **Tests**: `--delete-all-versions` deletes all versions including delete markers
    - **Setup**: Create versioned bucket; upload 10 objects; overwrite each object once (creates 2 versions each); delete some (creates delete markers)
    - **Config**: `--delete-all-versions --force`
    - **Assertions**: No object versions or delete markers remain; bucket is completely clean; stats count each version/marker as a separate deletion
    - _Requirements: 5.2, 5.4_

  - [ ] 29.23 `e2e_delete_all_versions_unversioned_bucket_error`
    - **Tests**: `--delete-all-versions` on a non-versioned bucket produces an error
    - **Setup**: Create standard (non-versioned) bucket; upload 5 objects
    - **Config**: `--delete-all-versions --force`
    - **Assertions**: Pipeline reports error about versioning requirement; no objects deleted
    - _Requirements: 5.2_

  ### E2E Test Plan: Callback Tests

  - [ ] 29.24 `e2e_rust_filter_callback`
    - **Tests**: Registering a Rust `FilterCallback` via `config.filter_manager.register_callback()`
    - **Setup**: Upload 20 objects: 10 with keys starting with `keep-`, 10 with keys starting with `delete-`
    - **Config**: Build config with `--force`; register a Rust filter callback that returns `true` only for keys starting with `delete-`
    - **Assertions**: 10 `delete-` objects removed; 10 `keep-` objects remain
    - _Requirements: 2.9, 12.5_

  - [ ] 29.25 `e2e_rust_event_callback`
    - **Tests**: Registering a Rust `EventCallback` via `config.event_manager.register_callback()`
    - **Setup**: Upload 10 objects
    - **Config**: Build config with `--force`; register a Rust event callback with `EventType::ALL_EVENTS` that collects events into an `Arc<Mutex<Vec<EventData>>>`
    - **Assertions**: Callback received `PIPELINE_START`, `DELETE_COMPLETE` (10 times), and `PIPELINE_END` events; event data includes correct keys and sizes
    - _Requirements: 7.6, 7.7, 12.6_

  - [ ] 29.26 `e2e_lua_filter_callback`
    - **Tests**: `--filter-callback-lua-script` with a Lua filter script
    - **Setup**: Upload 20 objects: 10 with `.tmp` extension, 10 with `.dat` extension. Write a temporary Lua filter script that returns `true` for keys ending in `.tmp`
    - **Config**: `--filter-callback-lua-script <path> --force`
    - **Assertions**: 10 `.tmp` objects deleted; 10 `.dat` objects remain
    - _Requirements: 2.8, 2.12_

  - [ ] 29.27 `e2e_lua_event_callback`
    - **Tests**: `--event-callback-lua-script` with a Lua event script
    - **Setup**: Upload 10 objects. Write a temporary Lua event script that writes event counts to a temp file (using `allow_lua_os_library`)
    - **Config**: `--event-callback-lua-script <path> --allow-lua-os-library --force`
    - **Assertions**: All 10 objects deleted; the Lua output file contains expected event records
    - _Requirements: 2.12, 7.6_

  - [ ] 29.28 `e2e_lua_sandbox_blocks_os_access`
    - **Tests**: Default Lua sandbox blocks OS library access
    - **Setup**: Upload 5 objects. Write a Lua filter script that calls `os.execute("echo test")`
    - **Config**: `--filter-callback-lua-script <path> --force` (no `--allow-lua-os-library`)
    - **Assertions**: Pipeline reports error (Lua sandbox violation); objects may or may not be deleted depending on when the error occurs
    - _Requirements: 2.13_

  - [ ] 29.29 `e2e_lua_vm_memory_limit`
    - **Tests**: `--lua-vm-memory-limit` enforces Lua memory cap
    - **Setup**: Upload 5 objects. Write a Lua filter script that allocates a large table (exceeding 1MB)
    - **Config**: `--filter-callback-lua-script <path> --lua-vm-memory-limit 1MB --force`
    - **Assertions**: Pipeline reports error or process terminates due to Lua memory limit exceeded
    - _Requirements: 2.14_

  - [ ] 29.30 `e2e_rust_filter_and_event_callbacks_combined`
    - **Tests**: Using both Rust filter and event callbacks simultaneously
    - **Setup**: Upload 20 objects: 10 large (5KB), 10 small (100B)
    - **Config**: Register a Rust filter callback (delete only objects >= 1KB); register a Rust event callback collecting events
    - **Assertions**: 10 large objects deleted; 10 small remain; event callback received DELETE_COMPLETE for each deleted object and DELETE_FILTERED for each skipped object
    - _Requirements: 2.9, 7.6, 12.5, 12.6_

  ### E2E Test Plan: Optimistic Locking Tests

  - [ ] 29.31 `e2e_if_match_single_deletion`
    - **Tests**: `--if-match` flag with single deletion mode (`--batch-size 1`)
    - **Setup**: Upload 10 objects
    - **Config**: `--if-match --batch-size 1 --force`
    - **Assertions**: All 10 objects deleted (ETags match since objects not modified between list and delete); stats show 10 deleted, 0 failed
    - _Requirements: 11.1, 11.3_

  - [ ] 29.32 `e2e_if_match_batch_deletion`
    - **Tests**: `--if-match` flag with batch deletion mode
    - **Setup**: Upload 20 objects
    - **Config**: `--if-match --batch-size 10 --force`
    - **Assertions**: All 20 objects deleted; ETags match for unmodified objects
    - _Requirements: 11.1, 11.4_

  ### E2E Test Plan: Performance Option Tests

  - [ ] 29.33 `e2e_worker_size_configuration`
    - **Tests**: `--worker-size` with different values
    - **Setup**: Upload 100 objects
    - **Config**: `--worker-size 4 --force`
    - **Assertions**: All 100 objects deleted; stats show 100 deleted
    - _Requirements: 1.3, 1.4_

  - [ ] 29.34 `e2e_rate_limit_objects`
    - **Tests**: `--rate-limit-objects` rate limiting
    - **Setup**: Upload 50 objects
    - **Config**: `--rate-limit-objects 200 --force` (200 obj/sec -- slow but should complete)
    - **Assertions**: All 50 objects deleted; pipeline completes (rate limiting does not prevent completion)
    - _Requirements: 8.7, 8.8_

  - [ ] 29.35 `e2e_max_parallel_listings`
    - **Tests**: `--max-parallel-listings` and `--max-parallel-listing-max-depth`
    - **Setup**: Upload 100 objects with varied prefixes (5 different top-level prefixes, 20 objects each)
    - **Config**: `--max-parallel-listings 2 --max-parallel-listing-max-depth 1 --force`
    - **Assertions**: All 100 objects deleted; parallel listing configuration did not cause errors
    - _Requirements: 1.5, 1.6, 1.7_

  - [ ] 29.36 `e2e_object_listing_queue_size`
    - **Tests**: `--object-listing-queue-size`
    - **Setup**: Upload 50 objects
    - **Config**: `--object-listing-queue-size 5 --force`
    - **Assertions**: All 50 objects deleted (small queue size does not prevent completion)
    - _Requirements: 1.8_

  - [ ] 29.37 `e2e_max_keys_listing`
    - **Tests**: `--max-keys` controls objects per list request
    - **Setup**: Upload 100 objects
    - **Config**: `--max-keys 10 --force` (forces pagination with 10 objects per page)
    - **Assertions**: All 100 objects deleted (pagination with small page size works correctly)
    - _Requirements: 1.5_

  ### E2E Test Plan: Tracing and Logging Tests

  - [ ] 29.38 `e2e_verbosity_levels`
    - **Tests**: `-v`, `-vv`, `-vvv` verbosity flags (grouped test)
    - **Setup**: Upload 5 objects
    - **Config**: Run 3 times with `-v`, `-vv`, `-vvv` respectively, all with `--force`
    - **Assertions**: All 5 objects deleted in each run; no errors; pipeline completes (logging configuration does not affect functionality)
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

  - [ ] 29.39 `e2e_json_tracing`
    - **Tests**: `--json-tracing` (requires `--force`)
    - **Setup**: Upload 5 objects
    - **Config**: `--json-tracing --force`
    - **Assertions**: Pipeline completes; all 5 objects deleted
    - _Requirements: 4.7, 13.3_

  - [ ] 29.40 `e2e_quiet_mode`
    - **Tests**: `-q` (quiet mode) suppresses progress
    - **Setup**: Upload 5 objects
    - **Config**: `-q --force`
    - **Assertions**: Pipeline completes; all 5 objects deleted
    - _Requirements: 7.4_

  - [ ] 29.41 `e2e_show_no_progress`
    - **Tests**: `--show-no-progress` hides progress bar
    - **Setup**: Upload 5 objects
    - **Config**: `--show-no-progress --force`
    - **Assertions**: Pipeline completes; all 5 objects deleted
    - _Requirements: 7.4_

  - [ ] 29.42 `e2e_disable_color_tracing`
    - **Tests**: `--disable-color-tracing`
    - **Setup**: Upload 5 objects
    - **Config**: `--disable-color-tracing --force`
    - **Assertions**: Pipeline completes; all 5 objects deleted
    - _Requirements: 4.8, 4.9_

  - [ ] 29.43 `e2e_log_deletion_summary`
    - **Tests**: `--log-deletion-summary`
    - **Setup**: Upload 10 objects
    - **Config**: `--log-deletion-summary --force`
    - **Assertions**: Pipeline completes; all 10 objects deleted; stats logged
    - _Requirements: 7.3_

  - [ ] 29.44 `e2e_aws_sdk_tracing_and_span_events`
    - **Tests**: `--aws-sdk-tracing` and `--span-events-tracing` (grouped)
    - **Setup**: Upload 5 objects
    - **Config**: `--aws-sdk-tracing --span-events-tracing -vvv --force`
    - **Assertions**: Pipeline completes; all 5 objects deleted; AWS SDK trace events produced
    - _Requirements: 4.5_

  ### E2E Test Plan: Retry and Timeout Option Tests

  - [ ] 29.45 `e2e_retry_options`
    - **Tests**: `--aws-max-attempts`, `--initial-backoff-milliseconds`, `--force-retry-count`, `--force-retry-interval-milliseconds`
    - **Setup**: Upload 10 objects
    - **Config**: `--aws-max-attempts 2 --initial-backoff-milliseconds 100 --force-retry-count 1 --force-retry-interval-milliseconds 100 --force`
    - **Assertions**: Pipeline completes; all 10 objects deleted (retry options do not prevent normal operation)
    - _Requirements: 6.1, 6.2_

  - [ ] 29.46 `e2e_timeout_options`
    - **Tests**: `--operation-timeout-milliseconds`, `--operation-attempt-timeout-milliseconds`, `--connect-timeout-milliseconds`, `--read-timeout-milliseconds`
    - **Setup**: Upload 10 objects
    - **Config**: `--operation-timeout-milliseconds 30000 --connect-timeout-milliseconds 5000 --read-timeout-milliseconds 5000 --force`
    - **Assertions**: Pipeline completes within timeout; all 10 objects deleted
    - _Requirements: 8.3_

  - [ ] 29.47 `e2e_disable_stalled_stream_protection`
    - **Tests**: `--disable-stalled-stream-protection`
    - **Setup**: Upload 10 objects
    - **Config**: `--disable-stalled-stream-protection --force`
    - **Assertions**: Pipeline completes; all 10 objects deleted
    - _Requirements: 8.3_

  ### E2E Test Plan: Error Handling and Access Denial Tests

  - [ ] 29.48 `e2e_access_denied_invalid_credentials`
    - **Tests**: Access denial with invalid AWS credentials
    - **Setup**: Create a standard bucket and upload 5 objects (using valid credentials for setup)
    - **Config**: Build config with `--target-access-key INVALID --target-secret-access-key INVALID --target-region us-east-1 --force`
    - **Assertions**: Pipeline reports error; `has_error()` returns true; error type is AWS SDK error (access denied or invalid credentials)
    - _Requirements: 6.4, 10.5, 13.4_

  - [ ] 29.49 `e2e_nonexistent_bucket_error`
    - **Tests**: Deleting from a bucket that does not exist
    - **Setup**: No bucket creation (use a random non-existent bucket name)
    - **Config**: `s3://s3rm-e2e-nonexistent-{uuid}/ --force`
    - **Assertions**: Pipeline reports error (NoSuchBucket or similar); `has_error()` returns true
    - _Requirements: 6.4, 10.5_

  - [ ] 29.50 `e2e_warn_as_error`
    - **Tests**: `--warn-as-error` promotes warnings to errors
    - **Setup**: Upload 10 objects
    - **Config**: `--warn-as-error --force`
    - **Assertions**: Pipeline completes; if any warnings occurred, `has_error()` returns true; otherwise normal completion
    - _Requirements: 10.5_

  ### E2E Test Plan: AWS Configuration Tests

  - [ ] 29.51 `e2e_target_region_override`
    - **Tests**: `--target-region` overrides profile region
    - **Setup**: Upload 10 objects in the default region
    - **Config**: `--target-region <same-region-as-bucket> --force`
    - **Assertions**: Pipeline completes; all 10 objects deleted (region override does not break access)
    - _Requirements: 8.5_

  - [ ] 29.52 `e2e_target_force_path_style`
    - **Tests**: `--target-force-path-style`
    - **Setup**: Upload 10 objects
    - **Config**: `--target-force-path-style --force`
    - **Assertions**: Pipeline completes; all 10 objects deleted (path-style access works with standard S3)
    - _Requirements: 8.6_

  - [ ] 29.53 `e2e_target_request_payer`
    - **Tests**: `--target-request-payer`
    - **Setup**: Upload 10 objects
    - **Config**: `--target-request-payer --force`
    - **Assertions**: Pipeline completes; all 10 objects deleted (request payer header does not break normal operations)
    - _Requirements: 8.3_

  - [ ] 29.53a `e2e_allow_lua_unsafe_vm`
    - **Tests**: `--allow-lua-unsafe-vm` disables Lua sandbox entirely
    - **Setup**: Upload 5 objects. Write a Lua filter script that uses `os.clock()` (OS library function)
    - **Config**: `--filter-callback-lua-script <path> --allow-lua-unsafe-vm --force`
    - **Assertions**: Pipeline completes without Lua sandbox error; all 5 matching objects deleted (unsafe VM allows OS access)
    - _Requirements: 2.14_

  ### E2E Test Plan: CLI Options Not Directly Testable in E2E

  The following CLI options are NOT tested in E2E because they require infrastructure not available in standard E2E test environments:
  - `--target-endpoint-url`: Requires a running S3-compatible service (MinIO/LocalStack). Out of scope for AWS E2E.
  - `--target-accelerate`: Requires S3 Transfer Acceleration enabled on the test bucket. Out of scope.
  - `--allow-parallel-listings-in-express-one-zone`: Requires Express One Zone directory bucket. Out of scope.
  - `--auto-complete-shell`: Shell completion generator, not a runtime feature. Covered by unit tests.
  - `--aws-config-file`, `--aws-shared-credentials-file`: Indirectly tested via `--target-profile s3rm-e2e-test` which uses the default credential file paths. Explicit path override testing is infrastructure-dependent.
  - `--target-session-token`: Conflicts with `--target-profile`; implicitly covered by 29.48 (credential group). Testing with STS temporary credentials requires an STS assume-role setup.

  ### E2E Test Plan: Combined/Advanced Tests

  - [ ] 29.54 `e2e_multiple_filters_combined`
    - **Tests**: Multiple filters combined with AND logic
    - **Setup**: Upload 30 objects: 10 with key `logs/small.txt` (100B), 10 with key `logs/large.txt` (10KB), 10 with key `data/large.dat` (10KB)
    - **Config**: `s3://{bucket}/logs/ --filter-include-regex "\\.txt$" --filter-larger-size 1KB --force`
    - **Assertions**: Only 10 `logs/large.txt` objects deleted (matching prefix AND regex AND size); 20 others remain
    - _Requirements: 2.11_

  - [ ] 29.55 `e2e_dry_run_with_delete_all_versions`
    - **Tests**: Dry-run mode combined with `--delete-all-versions`
    - **Setup**: Create versioned bucket; upload 10 objects; overwrite each once (creates 20 versions total)
    - **Config**: `--dry-run --delete-all-versions --force`
    - **Assertions**: All 20 versions still exist (dry-run); stats show 20 "deleted" (simulated); no actual S3 deletions
    - _Requirements: 3.1, 5.2, 5.4_

  - [ ] 29.56 `e2e_dry_run_with_filters`
    - **Tests**: Dry-run combined with filters accurately simulates filtered deletion
    - **Setup**: Upload 20 objects: 10 matching `^temp/`, 10 not matching
    - **Config**: `--dry-run --filter-include-regex "^temp/" --force`
    - **Assertions**: All 20 objects still exist; stats show 10 "deleted" (the matching ones)
    - _Requirements: 3.1, 2.2_

  - [ ] 29.57 `e2e_max_delete_with_filters`
    - **Tests**: `--max-delete` interacts correctly with filters
    - **Setup**: Upload 50 objects: 30 matching `^to-delete/`, 20 not matching
    - **Config**: `--filter-include-regex "^to-delete/" --max-delete 5 --force`
    - **Assertions**: Exactly 5 of the `to-delete/` objects deleted; 25 `to-delete/` and 20 other objects remain
    - _Requirements: 3.6, 2.2_

  - [ ] 29.58 `e2e_lua_filter_with_event_callback`
    - **Tests**: Lua filter callback combined with Lua event callback
    - **Setup**: Upload 20 objects. Write Lua filter (delete objects with size > 500B). Write Lua event script (log events to file, requires `--allow-lua-os-library`)
    - **Config**: `--filter-callback-lua-script <filter> --event-callback-lua-script <event> --allow-lua-os-library --force`
    - **Assertions**: Only objects > 500B deleted; event script output file contains expected events
    - _Requirements: 2.8, 2.12, 7.6_

  - [ ] 29.59 `e2e_large_object_count`
    - **Tests**: Deleting close to the maximum allowed objects (1000)
    - **Setup**: Upload 1000 objects (each 1KB)
    - **Config**: `--force`
    - **Assertions**: All 1000 objects deleted; stats show 1000 deleted, 0 failed
    - _Requirements: 1.1, 1.8_

  ### E2E Test Plan: Statistics and Result Verification

  - [ ] 29.60 `e2e_deletion_stats_accuracy`
    - **Tests**: `get_deletion_stats()` returns accurate counts and byte totals
    - **Setup**: Upload 15 objects of known sizes (e.g., 5 x 1KB, 5 x 2KB, 5 x 5KB = total 40KB)
    - **Config**: `--force`
    - **Assertions**: `stats.stats_deleted_objects == 15`; `stats.stats_deleted_bytes == 40960` (exact byte count); `stats.stats_failed_objects == 0`; `stats.duration > 0`
    - _Requirements: 6.5, 7.1, 7.3_

  - [ ] 29.61 `e2e_event_callback_receives_all_event_types`
    - **Tests**: Event callback receives PIPELINE_START, DELETE_COMPLETE, STATS_REPORT, and PIPELINE_END
    - **Setup**: Upload 5 objects. Register Rust event callback collecting all events.
    - **Config**: `--force`
    - **Assertions**: Received exactly 1 PIPELINE_START, 5 DELETE_COMPLETE, at least 1 PIPELINE_END; DELETE_COMPLETE events contain key, size; PIPELINE_END event contains final stats
    - _Requirements: 7.6, 7.7_

  ### E2E Test Execution Instructions

  ```bash
  # 1. Configure AWS credentials for the e2e test profile
  aws configure --profile s3rm-e2e-test

  # 2. Run all E2E tests
  RUSTFLAGS="--cfg e2e_test" cargo test --all-features --test '*' -- --test-threads=8

  # 3. Run a specific E2E test
  RUSTFLAGS="--cfg e2e_test" cargo test --all-features --test e2e_basic -- e2e_basic_prefix_deletion

  # WARNING: These tests create and delete real AWS S3 resources.
  # Failed tests may leave behind buckets that need manual cleanup.
  # Bucket names follow the pattern: s3rm-e2e-*
  ```

  ### E2E Test File Organization

  ```
  tests/
  +-- common/
  |   +-- mod.rs             # TestHelper, bucket management, pipeline runner
  +-- e2e_filter.rs          # Tests 29.1-29.12 (filtering)
  +-- e2e_deletion.rs        # Tests 29.13-29.17 (core deletion modes)
  +-- e2e_safety.rs          # Tests 29.18-29.20 (safety features)
  +-- e2e_versioning.rs      # Tests 29.21-29.23 (versioning)
  +-- e2e_callback.rs        # Tests 29.24-29.30 (Lua + Rust callbacks)
  +-- e2e_optimistic.rs      # Tests 29.31-29.32 (if-match)
  +-- e2e_performance.rs     # Tests 29.33-29.37 (performance options)
  +-- e2e_tracing.rs         # Tests 29.38-29.44 (logging/tracing)
  +-- e2e_retry.rs           # Tests 29.45-29.47 (retry/timeout options)
  +-- e2e_error.rs           # Tests 29.48-29.50 (error handling, access denial)
  +-- e2e_aws_config.rs      # Tests 29.51-29.53 (AWS config options)
  +-- e2e_combined.rs        # Tests 29.54-29.59 (combined/advanced scenarios)
  +-- e2e_stats.rs           # Tests 29.60-29.61 (statistics verification)
  ```

  _Requirements: 1.1-1.11, 2.1-2.14, 3.1-3.6, 4.1-4.10, 5.1-5.5, 6.1-6.6, 7.1-7.7, 8.1-8.8, 10.1-10.7, 11.1-11.4, 12.1-12.9, 13.1-13.7_


- [ ] 30. Final Checkpoint - Pre-Release Validation
  - Run all unit tests: `cargo test --lib` (verify 100% pass rate)
  - Run all property-based tests: `cargo test --lib` (verify 100% pass rate)
  - Run all integration tests: `cargo test --test '*'`
  - Verify code coverage meets target (>95% line coverage)
  - Verify clippy has no warnings: `cargo clippy --all-targets --all-features`
  - Verify rustfmt is applied: `cargo fmt --all -- --check`
  - Verify security audit passes: `cargo audit && cargo deny check`
  - Verify manual integration tests pass (E2E with real S3)
  - Verify documentation builds: `cargo doc --no-deps`
  - Review all completed tasks and mark any remaining work
  - Ask the user if questions arise or if ready to proceed to release


- [ ] 31. Release Preparation
  - [ ] 31.1 Build release binaries
    - Set up cross-compilation toolchains
    - Build for Linux x86_64 (glibc): `cargo build --release --target x86_64-unknown-linux-gnu`
    - Build for Linux x86_64 (musl): `cargo build --release --target x86_64-unknown-linux-musl`
    - Build for Linux ARM64: `cargo build --release --target aarch64-unknown-linux-gnu`
    - Build for Windows x86_64: `cargo build --release --target x86_64-pc-windows-gnu`
    - Build for Windows aarch64: `cargo build --release --target aarch64-pc-windows-msvc`
    - Build for macOS x86_64: `cargo build --release --target x86_64-apple-darwin`
    - Build for macOS aarch64: `cargo build --release --target aarch64-apple-darwin`
    - Test each binary on target platform
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.8_

  - [ ] 31.2 Create release artifacts
    - Package binaries with README, LICENSE, and CHANGELOG
    - Create tar.gz archives for Unix platforms
    - Create zip archives for Windows
    - Generate SHA256 checksums for all binaries
    - Create release notes with feature highlights
    - Tag release in git: `git tag -a v0.1.0 -m "Initial release"`
    - _Requirements: N/A (release process)_

## Notes

- Tasks marked with `*` are optional test-related sub-tasks and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation at key milestones
- Property tests validate universal correctness properties (49 total properties)
- Unit tests validate specific examples and edge cases
- Maximum code reuse from s3sync (~90% of codebase)
- Library-first architecture: CLI is thin wrapper over library
- Target: 97%+ line coverage (matching s3sync's 97.88%)
- All 49 correctness properties must have property-based tests with minimum 100 iterations

## Implementation Status Summary

Tasks 1-28 complete (including Task 23 checkpoint, Task 24 unit tests, Task 25 property testing infrastructure, Task 27 documentation, Task 28 code quality: clippy, rustfmt, cargo-deny all pass). All merged to init_build. Test totals: 442 lib tests (all pass), 26 binary tests (all pass).

**Already implemented across Tasks 1-25** (infrastructure available for remaining tasks):
- AWS client setup, credentials, retry, rate limiting, tracing (Task 2)
- All core data types: S3Object, DeletionStats, DeletionError, DeletionEvent, S3Target (Task 3)
- Storage trait, S3 storage with versioning, conditional deletion, rate limiting (Task 4)
- ObjectLister with parallel pagination and versioned listing (Task 5)
- All filter stages: regex, size, time, Lua user-defined (Task 6)
- Lua VM integration with filter and event callbacks, sandbox security (Task 7)
- FilterCallback and EventCallback traits, FilterManager, EventManager (Tasks 7-8)
- BatchDeleter, SingleDeleter, ObjectDeleter with if_match, versioning, content-type/metadata/tag filters (Task 8)
- SafetyChecker with dry-run, confirmation prompts, force flag, non-TTY detection, max-delete threshold (Task 9)
- DeletionPipeline orchestrator connecting all stages: list → filter → delete → terminate (Task 10)
- Terminator stage for draining final pipeline output (Task 10)
- Progress reporter with indicatif, UI config helpers, moving averages (Task 11)
- Library API: root-level re-exports, rustdoc documentation, property tests for API surface (Task 12)
- CLI binary: fully implemented with clap, tracing, progress indicator, Ctrl+C handler (Task 13)
- Versioning property tests: Properties 25-28 covering lister dispatch, deletion stage, version info (Task 14)
- Retry/error handling property tests: Properties 29-30 covering retry config, error classification, failure tracking (Task 15)
- Optimistic locking property tests: Properties 41-43 covering If-Match flag, SingleDeleter/BatchDeleter ETags (Task 16)
- Logging/verbosity property tests: Properties 21-24 covering verbosity levels, JSON format, color control, error logging (Task 17)
- AWS configuration property tests: Properties 34-35 covering credential loading, custom endpoint support (Task 18)
- Rate limiting property tests: Property 36 covering rate limit CLI propagation and enforcement (Task 19)
- Cross-platform property tests: Property 37 covering S3 URI handling, path normalization (Task 20)
- CI/CD integration property tests: Properties 48-49 covering non-interactive detection, output stream separation (Task 21)
- CI pipeline for all target platforms (Task 1)
- All 49 property tests implemented (Properties 1-49)
- Comprehensive unit tests for all components (Task 24, all sub-tasks done in Tasks 3-13)
- Rustdoc documentation, examples (Lua filter/event, Rust library usage), and CONTRIBUTING.md (Task 27.2, 27.3, 27.5)
- CLI polish: unified README commands, content-type filter help, --json-tracing requires -f, batch-size in Performance, dry-run hint, last_modified in logs, rate-limit vs batch-size validation (Task 23)
- Infrastructure: build info (shadow-rs + build.rs), Dockerfile, SECURITY.md, PR template with AI notice (Task 23)
- Spec audit: requirements.md, design.md, structure.md updated to match implementation (Task 23)

**Sub-tasks already completed in later task groups** (done during Tasks 1-25):
- 14.1: Version handling in ObjectDeleter (done in Tasks 3, 5, 8)
- 14.2: Version display in dry-run mode — not needed; each version is a separate object in the pipeline
- 15.1: Retry policy integration (done in Task 2)
- 15.2: Failure tracking (done in Tasks 3, 8)
- 15.5: Error handling unit tests (done in Tasks 3, 8)
- 16.1-16.2: If-Match support and conditional failure handling (done in Tasks 2, 3, 8)
- 16.3-16.5: Optimistic locking property tests (done in Task 16)
- 17.1-17.2: Tracing integration and error logging (done in Tasks 2, 8, 9)
- 18.1-18.2: AWS credential loading and custom endpoint support (done in Task 2)
- 18.3-18.4: AWS configuration property tests (done in Task 18)
- 19.1: Rate limiter integration (done in Task 2)
- 20.2-20.3: Terminal detection and cross-platform builds (done in Tasks 1, 9)
- 21.1: Non-interactive environment detection (done in Task 9)
- 21.2: Output stream separation — all logs to stdout by default via tracing-subscriber (done in Tasks 2, 13)
- 21.3-21.4: CI/CD integration property tests (done in Task 21)
- 24.1-24.6: All unit tests (done in Tasks 3-13)
- 25.1-25.2: Property-based testing infrastructure (done in Tasks 3-9)
- 27.2: Rustdoc documentation (done in Task 23, commit c9bedf8)
- 27.3: Example scripts — filter.lua, event.lua, library_usage.rs (done in Task 23, commit c9bedf8)
- 27.5: CONTRIBUTING.md (done in Task 23, commit c9bedf8)

**Remaining work**:
- Task 29: Automated E2E Integration Testing (test plan documented -- 62 test cases across 14 test files; implementation not yet started; requires AWS credentials with `s3rm-e2e-test` profile)
- Task 30: Final Checkpoint / Pre-Release Validation (not yet started)
- Task 31: Release Preparation (not yet started)