# Implementation Plan: s3rm-rs

## Executive Summary

**Project Status**: In progress

## Overview

This implementation plan follows a phased approach that maximizes code reuse from s3sync (~90% of codebase). The architecture is library-first, with the CLI as a thin wrapper. The implementation focuses on streaming pipelines with stages connected by async channels, targeting comprehensive property-based testing coverage for all critical correctness properties.

**Current Achievement**: Tasks 1-2 complete. Project setup and core infrastructure established.

## Current Status

**Completed Phases**:
Phase 0: Project Setup (Task 1)
Phase 1: Core Infrastructure (Task 2)

## Tasks

- [x] 1. Project Setup and Foundation
  - Create Cargo workspace with s3rm-rs library and binary
  - Configure dependencies matching s3sync versions (AWS SDK 1.122.0, tokio 1.49, clap 4.5, mlua 0.11, proptest 1.10)
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
    - Implement Error enum with thiserror (AwsSdk, InvalidConfig, InvalidUri, InvalidRegex, LuaScript, Io, Cancelled, DryRun, PartialFailure, Pipeline)
    - Include exit_code() and is_retryable() methods
    - _Requirements: 6.4, 6.5, 10.5, 13.4_

  - [x] 3.4 Create deletion event types
    - Implement DeletionEvent enum (PipelineStart, ObjectDeleted, ObjectFailed, PipelineEnd, PipelineError)
    - _Requirements: 7.7, 7.8_

  - [x] 3.5 Create S3Target type
    - Implement S3Target struct (bucket, prefix, endpoint, region)
    - Implement parse() method for s3:// URI parsing
    - _Requirements: 2.1, 8.5, 8.6_


- [ ] 4. Implement Storage Layer (Reuse from s3sync)
  - [ ] 4.1 Copy Storage trait and S3 storage implementation
    - Copy storage/mod.rs with Storage trait
    - Copy storage/s3.rs with S3 storage implementation
    - Adapt storage/factory.rs to create S3 storage only (no local storage needed)
    - Include is_versioning_enabled() method
    - _Requirements: 5.1, 5.2, 5.3_

  - [ ] 4.2 Write unit tests for storage factory
    - Test S3 storage creation with various configurations
    - Test versioning detection
    - _Requirements: 5.1, 5.2, 5.3_


- [ ] 5. Implement Object Lister (Reuse from s3sync)
  - [ ] 5.1 Copy ObjectLister implementation
    - Copy lister/mod.rs with parallel pagination support
    - Support configurable Parallel_Lister_Count
    - Support Max_Parallel_Listing_Max_Depth option
    - Handle versioned bucket listing
    - _Requirements: 1.5, 1.6, 1.7, 5.3_

  - [ ] 5.2 Write property test for parallel listing
    - **Property 5: Parallel Listing Configuration**
    - **Validates: Requirements 1.5, 1.6, 1.7**


- [ ] 6. Implement Filter Stages (Reuse from s3sync)
  - [ ] 6.1 Copy filter infrastructure
    - Copy pipeline/filter/mod.rs with ObjectFilter trait
    - Copy pipeline/stage.rs with Stage struct (adapted for deletion - only target storage)
    - _Requirements: 2.11_

  - [ ] 6.2 Copy time filters
    - Copy pipeline/filter/mtime.rs with MtimeBeforeFilter and MtimeAfterFilter
    - _Requirements: 2.7_

  - [ ] 6.3 Copy size filters
    - Copy pipeline/filter/size.rs with SmallerSizeFilter and LargerSizeFilter
    - _Requirements: 2.6_

  - [ ] 6.4 Copy regex filters for keys
    - Copy pipeline/filter/regex.rs with IncludeRegexFilter and ExcludeRegexFilter
    - Note: Content-type, metadata, and tag filters are in ObjectDeleter (not separate stages)
    - _Requirements: 2.2_

  - [ ] 6.5 Copy user-defined Lua filter
    - Copy pipeline/filter/user_defined.rs with UserDefinedFilter
    - _Requirements: 2.8_

  - [ ] 6.6 Write property tests for filters
    - **Property 7: Prefix Filtering**
    - **Validates: Requirements 2.1**

  - [ ] 6.7 Write property test for regex filtering
    - **Property 8: Regex Filtering**
    - **Validates: Requirements 2.2, 2.3, 2.4, 2.5, 2.12**

  - [ ] 6.8 Write property test for size filtering
    - **Property 9: Size Range Filtering**
    - **Validates: Requirements 2.6**

  - [ ] 6.9 Write property test for time filtering
    - **Property 10: Time Range Filtering**
    - **Validates: Requirements 2.7**


- [ ] 7. Implement Lua Integration (Reuse from s3sync)
  - [ ] 7.1 Copy Lua VM and callback infrastructure
    - Copy lua/vm.rs with LuaScriptCallbackEngine
    - Support safe mode (no OS/I/O), allow_lua_os_library mode, and unsafe mode
    - Implement memory limits
    - _Requirements: 2.13, 2.14_

  - [ ] 7.2 Copy Lua filter callback
    - Copy lua/callbacks.rs with LuaFilterCallback
    - Implement FilterCallback trait
    - _Requirements: 2.8, 2.12_

  - [ ] 7.3 Copy Lua event callback
    - Copy lua/callbacks.rs with LuaEventCallback
    - Implement EventCallback trait
    - _Requirements: 7.7, 7.8, 2.12_

  - [ ] 7.4 Write property test for Lua filter callbacks
    - **Property 11: Lua Filter Callback Execution**
    - **Validates: Requirements 2.8**

  - [ ] 7.5 Write property test for Lua callback types
    - **Property 14: Lua Callback Type Support**
    - **Validates: Requirements 2.12**

  - [ ] 7.6 Write property test for Lua sandbox security
    - **Property 15: Lua Sandbox Security**
    - **Validates: Requirements 2.13, 2.14**


- [ ] 8. Implement Deletion Components (New)
  - [ ] 8.1 Implement BatchDeleter
    - Create pipeline/batch_deleter.rs
    - Implement Deleter trait with delete() method
    - Group objects into batches of up to 1000
    - Call S3 DeleteObjects API with version IDs when applicable
    - Parse response to identify successes and failures
    - Retry failed objects using retry policy
    - _Requirements: 1.1, 1.9, 5.5_

  - [ ] 8.2 Implement SingleDeleter
    - Create pipeline/single_deleter.rs
    - Implement Deleter trait with delete() method
    - Delete objects one at a time using S3 DeleteObject API
    - Apply retry policy for each deletion
    - _Requirements: 1.2_

  - [ ] 8.3 Implement ObjectDeleter worker
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

  - [ ] 8.4 Write property test for batch deletion API usage
    - **Property 1: Batch Deletion API Usage**
    - **Validates: Requirements 1.1, 5.5**

  - [ ] 8.5 Write property test for single deletion API usage
    - **Property 2: Single Deletion API Usage**
    - **Validates: Requirements 1.2**

  - [ ] 8.6 Write property test for concurrent worker execution
    - **Property 3: Concurrent Worker Execution**
    - **Validates: Requirements 1.3**

  - [ ] 8.7 Write property test for partial batch failure recovery
    - **Property 6: Partial Batch Failure Recovery**
    - **Validates: Requirements 1.9**

  - [ ] 8.8 Write unit tests for ObjectDeleter
    - Test content-type filtering with HeadObject
    - Test metadata filtering with HeadObject
    - Test tagging filtering with GetObjectTagging
    - Test filter combination (AND logic)
    - _Requirements: 2.3, 2.4, 2.5, 2.11_


- [ ] 9. Implement Safety Features (New)
  - [ ] 9.1 Implement SafetyChecker
    - Create safety/mod.rs with SafetyChecker struct
    - Implement check_before_deletion() method
    - Handle dry-run mode (return DryRun error)
    - Implement confirmation prompt (require exact "yes" input)
    - Skip prompts if force flag is set
    - Skip prompts if JSON logging is enabled (would corrupt output)
    - Skip prompts in non-TTY environments
    - Use println/print for prompts (not tracing)
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 13.1_

  - [ ] 9.2 Implement max-delete threshold checking
    - Add threshold check in SafetyChecker
    - Stop and require confirmation when count exceeds limit
    - Skip if force flag is set
    - _Requirements: 3.6_

  - [ ] 9.3 Implement deletion summary display
    - Display total object count and estimated storage size
    - Show before confirmation prompt
    - _Requirements: 3.5_

  - [ ] 9.4 Write property test for dry-run mode safety
    - **Property 16: Dry-Run Mode Safety**
    - **Validates: Requirements 3.1**

  - [ ] 9.5 Write property test for confirmation prompt validation
    - **Property 17: Confirmation Prompt Validation**
    - **Validates: Requirements 3.3**

  - [ ] 9.6 Write property test for force flag behavior
    - **Property 18: Force Flag Behavior**
    - **Validates: Requirements 3.4, 13.2**

  - [ ] 9.7 Write property test for deletion summary display
    - **Property 19: Deletion Summary Display**
    - **Validates: Requirements 3.5**

  - [ ] 9.8 Write property test for threshold-based confirmation
    - **Property 20: Threshold-Based Additional Confirmation**
    - **Validates: Requirements 3.6**


- [ ] 10. Implement Deletion Pipeline (Adapted from s3sync)
  - [ ] 10.1 Create DeletionPipeline struct
    - Create pipeline/mod.rs with DeletionPipeline
    - Include config, target storage, cancellation_token, stats_receiver, error tracking
    - Adapt from s3sync's Pipeline structure
    - _Requirements: 1.8_

  - [ ] 10.2 Implement pipeline initialization
    - Implement new() method
    - Initialize storage using factory (S3 only)
    - Create stats channel
    - Initialize error tracking with Arc<AtomicBool> and Arc<Mutex<VecDeque<Error>>>
    - _Requirements: 1.8_

  - [ ] 10.3 Implement pipeline stages
    - Implement list_target() - spawn ObjectLister
    - Implement filter_objects() - chain filter stages
    - Implement delete_objects() - spawn ObjectDeleter workers (MPMC)
    - Implement terminate() - spawn Terminator
    - Connect stages with async channels (bounded)
    - _Requirements: 1.3, 1.5, 2.11_

  - [ ] 10.4 Implement pipeline run() method
    - Check prerequisites (versioning, dry-run, confirmation)
    - Create and connect all stages
    - Wait for completion
    - Handle cancellation via cancellation_token
    - _Requirements: 3.1, 3.2, 5.1_

  - [ ] 10.5 Implement error and statistics methods
    - Implement has_error(), get_errors_and_consume(), has_warning()
    - Implement get_deletion_stats()
    - Implement close_stats_sender()
    - _Requirements: 6.4, 6.5, 7.3_

  - [ ] 10.6 Copy Terminator implementation
    - Copy pipeline/terminator.rs from s3sync
    - Consume final stage output and close channels
    - _Requirements: N/A (infrastructure)_

  - [ ] 10.7 Write unit tests for pipeline stages
    - Test stage creation and connection
    - Test channel communication between stages
    - Test cancellation handling
    - _Requirements: 1.3, 1.8_


- [ ] 11. Implement Progress Reporting (Reuse from s3sync)
  - [ ] 11.1 Copy progress reporter
    - Copy progress/mod.rs with show_indicator() function
    - Use indicatif for progress bar
    - Calculate moving averages for throughput
    - Display ETA based on current rate
    - Display final summary with statistics
    - Support quiet mode
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_

  - [ ] 11.2 Write property test for progress reporting
    - **Property 31: Progress Reporting**
    - **Validates: Requirements 7.1, 7.3, 7.4, 7.5**

  - [ ] 11.3 Write property test for event callback invocation
    - **Property 32: Event Callback Invocation**
    - **Validates: Requirements 7.7, 7.8**


- [ ] 12. Implement Library API (Public Interface)
  - [ ] 12.1 Create lib.rs with public API
    - Export DeletionPipeline, Config, ParsedArgs
    - Export parse_from_args() function
    - Export create_pipeline_cancellation_token() function
    - Export DeletionStats struct
    - Export FilterCallback and EventCallback traits
    - Follow s3sync's API pattern exactly
    - _Requirements: 12.1, 12.2, 12.3, 12.4_

  - [ ] 12.2 Implement callback traits for Rust API
    - Define FilterCallback trait with filter() method
    - Define EventCallback trait with on_event() method
    - Support registration via Config
    - _Requirements: 12.5, 12.6_

  - [ ] 12.3 Add rustdoc documentation
    - Document all public types and functions
    - Include usage examples in doc comments
    - Document library-first architecture
    - _Requirements: 12.9_

  - [ ] 12.4 Write property test for library API configuration
    - **Property 44: Library API Configuration**
    - **Validates: Requirements 12.4**

  - [ ] 12.5 Write property test for library callback registration
    - **Property 45: Library Callback Registration**
    - **Validates: Requirements 12.5, 12.6**

  - [ ] 12.6 Write property test for library Lua callback support
    - **Property 46: Library Lua Callback Support**
    - **Validates: Requirements 12.7**

  - [ ] 12.7 Write property test for library async result handling
    - **Property 47: Library Async Result Handling**
    - **Validates: Requirements 12.8**


- [ ] 13. Implement CLI (Adapted from s3sync)
  - [ ] 13.1 Define CLI arguments with clap
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

  - [ ] 13.2 Implement parse_from_args function
    - Parse CLI arguments using clap
    - Handle environment variables
    - Apply configuration precedence (CLI > env > defaults)
    - Return ParsedArgs struct
    - Reuse s3sync's parsing pattern
    - _Requirements: 8.1, 8.2, 10.2_

  - [ ] 13.3 Implement Config::try_from(ParsedArgs)
    - Validate all arguments
    - Build Config struct
    - Apply defaults
    - Handle batch_size special case (Express One Zone defaults to 1)
    - Reuse s3sync's validation logic
    - _Requirements: 8.3, 10.2_

  - [ ] 13.4 Implement main.rs CLI entry point
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

  - [ ] 13.5 Write property test for configuration precedence
    - **Property 33: Configuration Precedence**
    - **Validates: Requirements 8.1, 8.2, 8.3, 8.5**

  - [ ] 13.6 Write property test for input validation
    - **Property 38: Input Validation and Error Messages**
    - **Validates: Requirements 10.2**

  - [ ] 13.7 Write property test for flag alias support
    - **Property 39: Flag Alias Support**
    - **Validates: Requirements 10.4**

  - [ ] 13.8 Write property test for exit code mapping
    - **Property 40: Exit Code Mapping**
    - **Validates: Requirements 10.5, 13.4**


- [ ] 14. Implement Versioning Support
  - [ ] 14.1 Add version handling to ObjectDeleter
    - Support delete marker creation for current versions
    - Support all-versions deletion when flag is set
    - Include version IDs in deletion requests
    - _Requirements: 5.1, 5.2_

  - [ ] 14.2 Add version display to dry-run mode
    - Show version counts per object in dry-run output
    - _Requirements: 5.4_

  - [ ] 14.3 Write property test for versioned bucket delete marker creation
    - **Property 25: Versioned Bucket Delete Marker Creation**
    - **Validates: Requirements 5.1**

  - [ ] 14.4 Write property test for all-versions deletion
    - **Property 26: All-Versions Deletion**
    - **Validates: Requirements 5.2**

  - [ ] 14.5 Write property test for version information retrieval
    - **Property 27: Version Information Retrieval**
    - **Validates: Requirements 5.3**

  - [ ] 14.6 Write property test for versioned dry-run display
    - **Property 28: Versioned Dry-Run Display**
    - **Validates: Requirements 5.4**


- [ ] 15. Implement Retry and Error Handling
  - [ ] 15.1 Verify retry policy integration
    - Ensure RetryPolicy is used in BatchDeleter and SingleDeleter
    - Verify exponential backoff with jitter
    - Verify error classification (retryable vs non-retryable)
    - _Requirements: 6.1, 6.2, 6.6_

  - [ ] 15.2 Implement failure tracking
    - Track failed objects in DeletionStatsReport
    - Log failures at appropriate verbosity level
    - Continue processing after failures
    - _Requirements: 6.4, 6.5_

  - [ ] 15.3 Write property test for retry with exponential backoff
    - **Property 29: Retry with Exponential Backoff**
    - **Validates: Requirements 6.1, 6.2, 6.6**

  - [ ] 15.4 Write property test for failure tracking and continuation
    - **Property 30: Failure Tracking and Continuation**
    - **Validates: Requirements 6.4, 6.5**

  - [ ] 15.5 Write unit tests for error handling
    - Test retryable error classification
    - Test non-retryable error handling
    - Test partial failure scenarios
    - _Requirements: 6.1, 6.2, 6.4_


- [ ] 16. Implement Optimistic Locking Support
  - [ ] 16.1 Add If-Match support to deletion requests
    - Add if_match field to Config
    - Include If-Match header in DeleteObject requests
    - Include If-Match in DeleteObjects requests
    - Handle PreconditionFailed errors
    - _Requirements: 11.1, 11.2, 11.3_

  - [ ] 16.2 Handle conditional deletion failures
    - Log ETag mismatch failures
    - Skip objects with failed conditions
    - Track in failed count
    - _Requirements: 11.2_

  - [ ] 16.3 Write property test for If-Match conditional deletion
    - **Property 41: If-Match Conditional Deletion**
    - **Validates: Requirements 11.1, 11.2**

  - [ ] 16.4 Write property test for ETag input parsing
    - **Property 42: ETag Input Parsing**
    - **Validates: Requirements 11.3**

  - [ ] 16.5 Write property test for batch conditional deletion handling
    - **Property 43: Batch Conditional Deletion Handling**
    - **Validates: Requirements 11.4**


- [ ] 17. Implement Logging and Verbosity
  - [ ] 17.1 Verify tracing integration
    - Ensure all components use tracing macros (trace!, debug!, info!, warn!, error!)
    - Verify verbosity level mapping (quiet=-1, normal=0, -v=1, -vv=2, -vvv=3)
    - Verify JSON and text format support
    - Verify color output control
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9_

  - [ ] 17.2 Implement error logging
    - Log deletion failures with error code and message
    - Include object key and context
    - Respect verbosity level
    - _Requirements: 4.10_

  - [ ] 17.3 Write property test for verbosity level configuration
    - **Property 21: Verbosity Level Configuration**
    - **Validates: Requirements 4.1**

  - [ ] 17.4 Write property test for JSON logging format
    - **Property 22: JSON Logging Format**
    - **Validates: Requirements 4.7, 13.3**

  - [ ] 17.5 Write property test for color output control
    - **Property 23: Color Output Control**
    - **Validates: Requirements 4.8, 4.9, 7.6, 13.7**

  - [ ] 17.6 Write property test for error logging
    - **Property 24: Error Logging**
    - **Validates: Requirements 4.10**


- [ ] 18. Implement AWS Configuration Support
  - [ ] 18.1 Verify AWS credential loading
    - Ensure credentials load from environment variables
    - Ensure credentials load from credentials file
    - Ensure credentials load from IAM roles
    - Use AWS SDK's standard credential chain
    - _Requirements: 8.4, 13.5_

  - [ ] 18.2 Verify custom endpoint support
    - Support custom endpoint URL via config
    - Enable S3-compatible services (MinIO, Wasabi)
    - _Requirements: 8.6_

  - [ ] 18.3 Write property test for AWS credential loading
    - **Property 34: AWS Credential Loading**
    - **Validates: Requirements 8.4**

  - [ ] 18.4 Write property test for custom endpoint support
    - **Property 35: Custom Endpoint Support**
    - **Validates: Requirements 8.6**


- [ ] 19. Implement Rate Limiting
  - [ ] 19.1 Verify rate limiter integration
    - Ensure RateLimiter is used in deletion pipeline
    - Verify token bucket algorithm
    - Verify objects per second enforcement
    - _Requirements: 8.7_

  - [ ] 19.2 Write property test for rate limiting enforcement
    - **Property 36: Rate Limiting Enforcement**
    - **Validates: Requirements 8.7**


- [ ] 20. Implement Cross-Platform Support
  - [ ] 20.1 Verify path handling
    - Test file path normalization on Windows
    - Test file path normalization on Unix
    - Ensure Lua script paths work cross-platform
    - _Requirements: 9.6_

  - [ ] 20.2 Verify terminal feature detection
    - Test TTY detection on different platforms
    - Test color output on different terminals
    - Test progress bar rendering
    - _Requirements: 9.7_

  - [ ] 20.3 Configure cross-platform builds
    - Set up CI for Linux x86_64 (glibc and musl)
    - Set up CI for Linux ARM64
    - Set up CI for Windows x86_64 and aarch64
    - Set up CI for macOS x86_64 and aarch64
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.8_

  - [ ] 20.4 Write property test for cross-platform path handling
    - **Property 37: Cross-Platform Path Handling**
    - **Validates: Requirements 9.6**


- [ ] 21. Implement CI/CD Integration Features
  - [ ] 21.1 Implement non-interactive environment detection
    - Detect absence of TTY using atty crate
    - Disable interactive prompts in non-TTY environments
    - _Requirements: 13.1_

  - [ ] 21.2 Verify output stream separation
    - Ensure errors go to stderr
    - Ensure structured logs go to stdout
    - _Requirements: 13.6_

  - [ ] 21.3 Write property test for non-interactive environment detection
    - **Property 48: Non-Interactive Environment Detection**
    - **Validates: Requirements 13.1**

  - [ ] 21.4 Write property test for output stream separation
    - **Property 49: Output Stream Separation**
    - **Validates: Requirements 13.6**


- [ ] 22. Implement Additional Property Tests
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


- [ ] 24. Write Comprehensive Unit Tests
  - [ ] 24.1 Write unit tests for configuration parsing
    - Test CLI argument parsing with various combinations
    - Test environment variable precedence
    - Test configuration validation
    - Test invalid input handling
    - _Requirements: 8.1, 8.2, 8.3, 10.2_

  - [ ] 24.2 Write unit tests for filter logic
    - Test regex pattern matching edge cases
    - Test size range boundary conditions
    - Test time range boundary conditions
    - Test filter combination (AND logic)
    - _Requirements: 2.2, 2.6, 2.7, 2.11_

  - [ ] 24.3 Write unit tests for deletion logic with mocks
    - Test batch deletion grouping (1-1000 objects)
    - Test single deletion
    - Test partial failure handling
    - Test retry logic with mock AWS client
    - _Requirements: 1.1, 1.2, 1.9, 6.1, 6.2_

  - [ ] 24.4 Write unit tests for safety features
    - Test dry-run mode (no actual deletions)
    - Test confirmation prompt parsing
    - Test force flag behavior
    - Test threshold checking
    - _Requirements: 3.1, 3.3, 3.4, 3.6_

  - [ ] 24.5 Write unit tests for versioning support
    - Test delete marker creation
    - Test all-versions deletion
    - Test version ID handling in requests
    - _Requirements: 5.1, 5.2, 5.5_

  - [ ] 24.6 Write unit tests for error handling
    - Test error classification (retryable vs non-retryable)
    - Test exit code mapping
    - Test partial failure scenarios
    - _Requirements: 6.1, 6.4, 10.5, 13.4_


- [ ] 25. Set Up Property-Based Testing Infrastructure
  - [ ] 25.1 Review existing property test generators
    - Verify arbitrary_s3_object() generator exists and is complete
    - Verify arbitrary_datetime() generator exists and is complete
    - Verify arbitrary_regex_pattern() generator exists and is complete
    - Verify arbitrary_config() generator exists and is complete
    - Verify proptest configured with appropriate iteration counts
    - _Requirements: N/A (testing infrastructure)_

  - [ ] 25.2 Create additional test utilities if needed
    - Implement mock AWS client for testing (if not already present)
    - Implement test fixtures for common scenarios (if not already present)
    - Implement assertion helpers (if not already present)
    - _Requirements: N/A (testing infrastructure)_


- [ ] 26. Verify All Property Tests Are Implemented
  - Review all 49 correctness properties from design document
  - Verify each property has a corresponding property-based test file
  - Verify each test runs with appropriate iteration counts
  - Verify each test includes comment tag: "Feature: s3rm-rs, Property N: [property text]"
  - Run all property tests: `cargo test --lib`
  - Document any missing property tests
  - _Requirements: All requirements (comprehensive coverage)_


**Implemented Property Tests**: None yet.

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
    - _Requirements: 2.8, 7.7, 12.1_

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

Task 1 complete: Project setup with Cargo.toml, src/lib.rs, src/bin/s3rm/main.rs, .gitignore, and CI pipeline (.github/workflows/ci.yml).

Task 2 complete: Core infrastructure reused from s3sync - types (S3rmObject, S3Credentials, AccessKeys with zeroize), config (Config, ClientConfig, RetryConfig, CLITimeoutConfig, TracingConfig, FilterConfig, ForceRetryConfig), S3 client builder (credential loading, region, endpoint, retry, timeout), tracing (init_tracing with verbosity/JSON/color/AWS SDK), and cancellation token. 23 tests pass, 0 clippy warnings.