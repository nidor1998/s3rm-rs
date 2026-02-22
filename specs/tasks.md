# Implementation Plan: s3rm-rs

## Executive Summary

**Project Status**: In progress

## Overview

This implementation plan follows a phased approach that maximizes code reuse from s3sync (~90% of codebase). The architecture is library-first, with the CLI as a thin wrapper. The implementation focuses on streaming pipelines with stages connected by async channels, targeting comprehensive property-based testing coverage for all critical correctness properties.

**Current Achievement**: Tasks 1-21 complete (plus Tasks 24-25). Project setup, core infrastructure, core data models, storage layer, object lister, filter stages, Lua integration, deletion components, safety features, deletion pipeline, progress reporting, library API, CLI implementation, versioning support, retry/error handling, optimistic locking, logging/verbosity, AWS configuration property tests, rate limiting property tests, cross-platform support, and CI/CD integration established.

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


- [-] 22. Implement Additional Property Tests
  - [ ] 22.1 Write property test for worker count configuration validation
    - **Property 4: Worker Count Configuration Validation**
    - **Validates: Requirements 1.4**

  - [ ] 22.2 Write property test for Rust filter callback execution
    - **Property 12: Rust Filter Callback Execution**
    - **Validates: Requirements 2.9**

  - [ ] 22.3 Write property test for delete-all behavior
    - **Property 13: Delete-All Behavior**
    - **Validates: Requirements 2.10**


- [ ] 23. Checkpoint - Review Current Implementation
  - Review all completed implementation tasks
  - Verify all core deletion functionality is working
  - Verify all filter stages are connected correctly
  - Verify safety features are working (dry-run, confirmation)
  - Run existing property tests and verify they pass
  - Identify any gaps or issues
  - Ask the user if questions arise


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


- [ ] 26. Verify All Property Tests Are Implemented
  - Review all 49 correctness properties from design document
  - Verify each property has a corresponding property-based test file
  - Verify each test runs with appropriate iteration counts
  - Verify each test includes comment tag: "Feature: s3rm-rs, Property N: [property text]"
  - Run all property tests: `cargo test --lib`
  - Document any missing property tests
  - _Requirements: All requirements (comprehensive coverage)_


**Implemented Property Tests**: Properties 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 14, 15, 16, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49 (44 of 49). Missing: 4, 12, 13, 19, 20.

- [ ] 27. Documentation and Examples
  - [ ] 27.1 Write README.md
    - Include project overview and features
    - Include installation instructions
    - Include usage examples (CLI and library)
    - Include CLI reference with common options
    - Include library usage examples
    - Include links to documentation
    - _Requirements: 10.1, 10.7, 12.9_

  - [ ] 27.2 Write rustdoc documentation
    - Document all public API types and functions in lib.rs
    - Include code examples in doc comments
    - Document library-first architecture
    - Document callback registration (Rust and Lua)
    - Document configuration options
    - Run `cargo doc --open` to verify
    - _Requirements: 12.9_

  - [ ] 27.3 Create example scripts
    - Create example Lua filter script (examples/filter.lua)
    - Create example Lua event callback script (examples/event.lua)
    - Create example Rust library usage (examples/library_usage.rs)
    - Document each example with comments
    - _Requirements: 2.8, 7.6, 12.1_

  - [ ] 27.4 Write CHANGELOG.md
    - Document initial release features
    - Document differences from s3sync
    - Document breaking changes (if any)
    - _Requirements: N/A (documentation)_

  - [ ] 27.5 Write CONTRIBUTING.md
    - Document development setup
    - Document testing requirements
    - Document code style guidelines
    - Document PR process
    - _Requirements: N/A (documentation)_


- [ ] 28. Code Quality and Coverage
  - [ ] 28.1 Run clippy and fix all warnings
    - Run `cargo clippy --all-targets --all-features`
    - Fix all clippy warnings
    - Ensure no clippy::pedantic warnings
    - _Requirements: N/A (code quality)_

  - [ ] 28.2 Run rustfmt and format all code
    - Run `cargo fmt --all`
    - Ensure consistent code style across all files
    - Verify formatting in CI
    - _Requirements: N/A (code quality)_

  - [ ] 28.3 Generate and verify test coverage
    - Install coverage tool: `cargo install cargo-tarpaulin` or `cargo install cargo-llvm-cov`
    - Run coverage: `cargo tarpaulin --out Html` or `cargo llvm-cov --html`
    - Verify line coverage >95% (target: match s3sync's 97.88%)
    - Verify branch coverage >90%
    - Identify and test uncovered code paths
    - Generate coverage report for documentation
    - _Requirements: N/A (testing goal)_

  - [ ] 28.4 Run security audit
    - Run `cargo audit` to check for known vulnerabilities
    - Run `cargo deny check` for license and security issues
    - Fix any security vulnerabilities
    - Document any accepted risks
    - _Requirements: N/A (security)_

  - [ ] 28.5 Run benchmarks (optional)
    - Create benchmark suite for critical paths
    - Benchmark batch deletion throughput
    - Benchmark filter performance
    - Document performance characteristics
    - _Requirements: 1.10 (performance goal)_


- [ ] 29. Integration Testing (Manual E2E)
  - [ ] 29.1 Test with real S3 bucket
    - Create test bucket with sample objects (various sizes, types)
    - Test basic deletion with prefix filter
    - Test batch deletion (verify 1000 objects per batch via logs)
    - Test single deletion mode (batch_size=1)
    - Test dry-run mode (verify no actual deletions)
    - Test versioned bucket operations (delete markers, all-versions)
    - Test with various filter combinations (regex, size, time)
    - Test confirmation prompts and force flag
    - Clean up test resources
    - _Requirements: 1.1, 1.2, 3.1, 5.1, 5.2_

  - [ ] 29.2 Test with S3-compatible services
    - Set up MinIO locally or use test instance
    - Test with LocalStack
    - Verify custom endpoint support works correctly
    - Test all core features with S3-compatible services
    - _Requirements: 8.6_

  - [ ] 29.3 Test performance at scale
    - Create bucket with 10,000+ objects
    - Test deletion throughput (measure objects/second)
    - Verify memory usage remains bounded during large deletions
    - Test with various worker counts (1, 10, 100, 1000)
    - Test with rate limiting enabled
    - Document performance characteristics
    - _Requirements: 1.10, 1.8, 8.7_

  - [ ] 29.4 Test cross-platform
    - Test on Linux x86_64 (glibc)
    - Test on Linux x86_64 (musl)
    - Test on Linux ARM64
    - Test on Windows x86_64
    - Test on macOS x86_64 and ARM64
    - Verify path handling on each platform
    - Verify terminal features on each platform
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.6, 9.7_

  - [ ] 29.5 Test CI/CD integration scenarios
    - Test in non-interactive environment (no TTY)
    - Test with JSON logging enabled
    - Test with environment variable configuration
    - Test exit codes for different scenarios
    - Test with force flag in automated scripts
    - _Requirements: 13.1, 13.2, 13.3, 13.4, 13.5, 13.6_


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

Tasks 1-21 complete (+ Tasks 24, 25 with all sub-tasks done). All merged to init_build.

**Already implemented across Tasks 1-21** (infrastructure available for remaining tasks):
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
- 44 property tests implemented (Properties 1-3, 5-11, 14-18, 21-37, 38-49)
- Comprehensive unit tests for all components (Task 24, all sub-tasks done in Tasks 3-13)

**Sub-tasks already completed in later task groups** (done during Tasks 1-21):
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

**Remaining work**:
- Task 22: Remaining property tests (5 of 49 properties still need tests: 4, 12, 13, 19, 20)
- Task 23: Checkpoint review
- Task 26: Verify all property tests
- Tasks 27-31: Documentation, quality, E2E testing, release