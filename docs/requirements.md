# Requirements Document: s3rm-rs

## Introduction

s3rm-rs is an Amazon S3 object deletion tool designed for bulk deletion operations. Built on the architectural patterns of s3sync, s3rm-rs leverages Rust's performance and the AWS SDK for Rust to provide reliable deletion of S3 objects with comprehensive safety features and versioning support.

The tool targets scenarios requiring deletion of thousands to millions of objects, such as data lifecycle management, bucket cleanup, and automated data retention policies. By utilizing S3's batch deletion API and parallel processing, s3rm-rs aims to achieve deletion rates of approximately 3,500 objects per second, which is the practical limit imposed by Amazon S3. The default configuration (worker-size 16, batch-size 200) is tuned to match this S3 throughput limit.

s3rm-rs is architected as a library-first design, where all core functionality is implemented in the s3rm library. The command-line interface is built on top of this library, ensuring that all features available in the CLI are also accessible programmatically.

## Glossary

- **S3rm_Tool**: The s3rm-rs command-line application and library
- **Deletion_Engine**: The core component responsible for orchestrating parallel deletion operations
- **Worker_Pool**: A collection of concurrent workers that execute deletion requests (1-65535 workers)
- **Batch_Deleter**: Component that groups objects into batches for the S3 DeleteObjects API
- **Single_Deleter**: Component that deletes objects one at a time using DeleteObject API
- **Object_Lister**: Component that retrieves object lists from S3 with parallel pagination
- **Audit_Logger**: Logging of deletion operations via tracing macros with verbosity controlled by -v flags (no separate component; integrated into each pipeline stage)
- **Dry_Run_Mode**: Execution mode that simulates deletions without actually removing objects
- **Delete_Marker**: S3 versioning concept - a placeholder indicating an object was deleted
- **Prefix**: S3 key prefix used to filter objects (e.g., "logs/2023/")
- **Regex_Pattern**: Regular expression pattern for matching object keys, content types, metadata, or tags
- **Batch_Size**: Number of objects grouped together for a single DeleteObjects API call (max 1000)
- **Worker_Count**: Number of concurrent workers processing deletion operations (1-65535)
- **Retry_Policy**: Strategy for handling transient failures with exponential backoff
- **Rate_Limiter**: Component that controls deletion throughput in objects per second
- **Filter_Callback_Lua**: User-provided Lua script for custom filtering logic (determines which objects to delete)
- **Filter_Callback_Rust**: User-provided Rust function for custom filtering logic
- **Event_Callback_Lua**: User-provided Lua script that receives events during deletion operations (progress, errors, completions)
- **Event_Callback_Rust**: User-provided Rust function that receives events during deletion operations
- **If_Match**: Boolean flag enabling ETag-based optimistic locking; when enabled, uses each object's own ETag for conditional deletions to prevent race conditions
- **Custom_Endpoint**: S3-compatible service endpoint (e.g., MinIO, Wasabi)
- **Parallel_Lister_Count**: Number of concurrent listing operations (configurable)
- **Max_Parallel_Listing_Max_Depth**: Maximum depth for parallel listing operations (similar to s3sync's --max-parallel-listing-max-depth)

## Requirements

### Requirement 1: High-Performance Parallel Deletion

**User Story:** As a DevOps engineer, I want to delete large numbers of S3 objects quickly with minimal memory usage, so that I can efficiently manage storage costs and data lifecycle policies.

#### Acceptance Criteria

1. WHERE batch deletion mode is selected, THE Deletion_Engine SHALL utilize the S3 DeleteObjects batch API to delete up to 1000 objects per request
2. WHERE single deletion mode is selected, THE Deletion_Engine SHALL utilize the S3 DeleteObject API to delete objects one at a time
3. WHEN processing deletions, THE Worker_Pool SHALL execute multiple deletion requests concurrently
4. THE S3rm_Tool SHALL support configurable worker count from 1 to 65535 workers
5. WHEN listing objects for deletion, THE Object_Lister SHALL use parallel pagination with configurable Parallel_Lister_Count
6. THE S3rm_Tool SHALL allow configuration of the number of parallel listing operations
7. THE S3rm_Tool SHALL support Max_Parallel_Listing_Max_Depth option to control the maximum depth for parallel listing operations
8. THE Deletion_Engine SHALL minimize memory usage by streaming object lists rather than loading all objects into memory
9. WHEN batch operations fail, THE Batch_Deleter SHALL extract successfully deleted objects and retry only the failed ones
10. THE Deletion_Engine SHALL target a performance goal of approximately 3,500 objects per second, which is the practical limit imposed by Amazon S3
11. WHERE the target is an Express One Zone directory bucket (detected by the `--x-s3` bucket name suffix), THE S3rm_Tool SHALL automatically set batch_size to 1 and disable parallel listing unless explicitly overridden via --allow-parallel-listings-in-express-one-zone

### Requirement 2: Flexible Filtering and Selection

**User Story:** As a user, I want powerful filtering capabilities to specify which objects to delete, so that I can handle complex deletion scenarios efficiently.

#### Acceptance Criteria

1. WHEN provided a prefix argument, THE S3rm_Tool SHALL delete all objects matching that prefix
2. WHEN provided a regex pattern for keys, THE S3rm_Tool SHALL delete all objects whose keys match the regular expression
3. WHEN provided a regex pattern for ContentType, THE S3rm_Tool SHALL delete all objects whose ContentType matches the regular expression
4. WHEN provided a regex pattern for user-defined metadata, THE S3rm_Tool SHALL delete all objects whose metadata matches the regular expression
5. WHEN provided a regex pattern for tags, THE S3rm_Tool SHALL delete all objects whose tags match the regular expression
6. WHEN provided size filters (min/max), THE S3rm_Tool SHALL delete only objects within the specified size range
7. WHEN provided modified time filters, THE S3rm_Tool SHALL delete only objects within the specified time range
8. WHEN provided a filter callback Lua script, THE S3rm_Tool SHALL execute the Filter_Callback_Lua for each object and delete objects where the callback returns true
9. WHERE a Rust filter callback is registered via the library API, THE S3rm_Tool SHALL execute the Filter_Callback_Rust for custom filtering logic
10. WHEN invoked with a bucket-only target (e.g. `s3://bucket/`), THE S3rm_Tool SHALL delete all objects in the specified bucket
11. WHERE multiple selection criteria are provided, THE S3rm_Tool SHALL combine them using logical AND operations
12. THE S3rm_Tool SHALL support Lua callback types: filter and event callbacks
13. WHERE Lua scripts are provided, THE S3rm_Tool SHALL run them in safe mode by default (no OS or I/O library access)
14. THE S3rm_Tool SHALL provide options to allow Lua OS library and unsafe VM operations when explicitly enabled

### Requirement 3: Safety and Confirmation Features

**User Story:** As a user, I want safeguards against accidental data loss, so that I can confidently use the tool without fear of mistakes.

#### Acceptance Criteria

1. WHEN dry-run mode is enabled, THE S3rm_Tool SHALL run the full pipeline (listing, filtering) but simulate deletions without making actual S3 API calls, logging each object that would be deleted at info level with a `[dry-run]` prefix, and outputting deletion statistics
2. WHEN performing destructive operations without dry-run mode, THE S3rm_Tool SHALL prompt for user confirmation before proceeding, displaying a warning message with the target prefix (e.g., "WARNING: All objects matching prefix s3://bucket/prefix will be deleted.")
3. WHERE the confirmation prompt is active, THE S3rm_Tool SHALL require explicit "yes" input and reject abbreviated responses
4. WHEN the force flag is provided, THE S3rm_Tool SHALL skip confirmation prompts
5. THE S3rm_Tool SHALL display the target prefix (e.g. `s3://bucket/prefix`) with colored text in the confirmation prompt so users can verify which objects will be affected. The prompt SHALL also include a hint about --dry-run mode for previewing deletions. Object count and size estimation is available via dry-run mode.
6. WHERE the --max-delete option is specified, THE S3rm_Tool SHALL cancel the pipeline at deletion time when the deletion count exceeds the specified limit (enforced in ObjectDeleter, similar to s3sync's --max-delete)

### Requirement 4: Comprehensive Logging

**User Story:** As a user, I want detailed logs of deletion operations with configurable verbosity, so that I can monitor operations at the appropriate level of detail for my needs.

#### Acceptance Criteria

1. THE S3rm_Tool SHALL support verbosity levels controlled by -v, -vv, and -vvv flags
2. WHEN no verbosity flag is provided, THE S3rm_Tool SHALL output minimal progress information
3. WHEN -v flag is provided, THE S3rm_Tool SHALL output standard operational logs including object keys and deletion status
4. WHEN -vv flag is provided, THE S3rm_Tool SHALL output detailed logs including timestamps, batch information, and retry attempts
5. WHEN -vvv flag is provided, THE S3rm_Tool SHALL output trace-level logs including API request details and internal state
6. THE S3rm_Tool SHALL output logs in human-readable text format by default
7. WHERE JSON logging is enabled, THE S3rm_Tool SHALL output structured JSON format for all log levels. Note: --json-tracing requires -f/--force because JSON output is incompatible with interactive confirmation prompts
8. WHERE color output is enabled (default), THE S3rm_Tool SHALL use colored text for improved readability
9. THE S3rm_Tool SHALL support disabling colored output via command-line flag or environment variable
10. WHEN a deletion fails, THE S3rm_Tool SHALL log the error message and error code at the current verbosity level

### Requirement 5: S3 Versioning Support

**User Story:** As a user working with versioned buckets, I want control over version deletion, so that I can manage object versions according to my retention policies.

#### Acceptance Criteria

1. WHEN deleting from a versioned bucket without version specification, THE S3rm_Tool SHALL create delete markers for current versions
2. WHERE the delete-all-versions flag is provided, THE S3rm_Tool SHALL delete all versions of matching objects including delete markers
3. THE Object_Lister SHALL retrieve version information when operating on versioned buckets
4. WHEN displaying dry-run results for versioned buckets, THE S3rm_Tool SHALL count each version as a separate object in progress and summary statistics
5. THE Batch_Deleter SHALL handle version IDs correctly in DeleteObjects API requests
6. WHERE the delete-all-versions flag is provided but the target bucket does not have versioning enabled, THE S3rm_Tool SHALL silently ignore the flag and proceed with normal deletion of all matching objects

### Requirement 6: Robust Error Handling and Retry Logic

**User Story:** As a user, I want the tool to handle transient failures gracefully, so that temporary network issues don't cause incomplete deletions.

#### Acceptance Criteria

1. WHEN an API request fails with a retryable error (5xx, throttling), THE Retry_Policy SHALL retry the request with exponential backoff
2. THE Retry_Policy SHALL implement exponential backoff with configurable maximum retry attempts
3. WHEN a batch deletion partially fails, THE Batch_Deleter SHALL identify failed objects and retry them in subsequent batches
4. IF an object deletion fails after all retries, THE S3rm_Tool SHALL log the failure and continue processing remaining objects
5. THE S3rm_Tool SHALL track and report the count of successfully deleted objects versus failed deletions
6. WHEN rate limiting occurs (SlowDown error), THE Retry_Policy SHALL apply exponential backoff before retrying

### Requirement 7: Progress Reporting and Statistics

**User Story:** As a user, I want real-time feedback on deletion progress with clear visual indicators, so that I can monitor long-running operations.

#### Acceptance Criteria

1. WHEN deletions are in progress, THE S3rm_Tool SHALL display a progress indicator showing objects deleted, deletion rate, and byte count
2. THE S3rm_Tool SHALL update progress statistics at least once per second
3. WHEN operations complete, THE S3rm_Tool SHALL display summary statistics including total objects deleted, duration, and throughput
4. THE S3rm_Tool SHALL support quiet mode that suppresses progress output for scripting scenarios
5. WHERE color output is enabled (default), THE S3rm_Tool SHALL use colored text for progress indicators and statistics
6. WHERE event callbacks are registered (Lua or Rust), THE S3rm_Tool SHALL invoke Event_Callback_Lua or Event_Callback_Rust for progress updates, errors, and completion events
7. THE event callbacks SHALL receive structured event data including event type, object key, status, timestamps, and error information when applicable

### Requirement 8: Configuration and Customization

**User Story:** As a user, I want to customize tool behavior for my specific use case, so that I can optimize performance and safety for my environment.

#### Acceptance Criteria

1. THE S3rm_Tool SHALL support configuration via command-line arguments and environment variables
2. WHERE multiple configuration sources are present, THE S3rm_Tool SHALL apply precedence: CLI args > environment variables > defaults
3. THE S3rm_Tool SHALL allow configuration of worker count, batch size, retry attempts, and timeout values
4. THE S3rm_Tool SHALL support AWS credential configuration through standard AWS SDK mechanisms (environment, credentials file, IAM roles)
5. THE S3rm_Tool SHALL allow specification of AWS region via command-line or environment variables
6. WHERE a custom endpoint is specified, THE S3rm_Tool SHALL support S3-compatible services (MinIO, Wasabi, etc.)
7. WHERE rate limiting is configured, THE Rate_Limiter SHALL enforce maximum objects per second deletion rate
8. WHERE rate limiting is configured, THE S3rm_Tool SHALL validate that --rate-limit-objects is greater than or equal to --batch-size, since a single batch operation must not exceed the rate limit

### Requirement 9: Cross-Platform Support

**User Story:** As a user on different operating systems, I want the tool to work consistently, so that I can use it across my infrastructure.

#### Acceptance Criteria

1. THE S3rm_Tool SHALL compile and run on x86_64 Linux with glibc (kernel 3.2 or later)
2. THE S3rm_Tool SHALL compile and run on x86_64 Linux with musl libc (kernel 3.2 or later)
3. THE S3rm_Tool SHALL compile and run on ARM64 Linux (kernel 4.1 or later)
4. THE S3rm_Tool SHALL compile and run on Windows 11 (x86_64 and aarch64)
5. THE S3rm_Tool SHALL compile and run on macOS 11.0 or later (x86_64 and aarch64)
6. THE S3rm_Tool SHALL handle file paths correctly across different operating systems
7. THE S3rm_Tool SHALL use platform-appropriate terminal features for progress display
8. THE S3rm_Tool SHALL provide pre-built binaries for all supported platforms

### Requirement 10: Command-Line Interface Design

**User Story:** As a user, I want an intuitive and easy-to-use command-line interface, so that I can quickly learn and use the tool effectively.

#### Acceptance Criteria

1. THE S3rm_Tool SHALL provide a help command that displays usage information and examples
2. THE S3rm_Tool SHALL validate all required arguments and provide clear error messages for invalid input
3. WHEN invoked without required arguments, THE S3rm_Tool SHALL display usage information
4. THE S3rm_Tool SHALL support both short flags (-d, -f) and long flags (--dry-run, --force) for commonly-used options, with all other options available via long flags only
5. THE S3rm_Tool SHALL return appropriate exit codes: 0 for success, 1 for errors, 2 for invalid arguments, 3 for warnings (partial failure), and 101 for abnormal termination (internal panic)
6. THE S3rm_Tool SHALL provide version information via --version flag
7. THE S3rm_Tool SHALL provide an intuitive command structure that is easy to learn and remember

### Requirement 11: Optimistic Locking Support

**User Story:** As a user managing concurrent operations, I want optimistic locking support, so that I can prevent race conditions when deleting objects.

#### Acceptance Criteria

1. WHERE the --if-match flag is enabled, THE S3rm_Tool SHALL use each object's own ETag (obtained during listing) to include the If-Match header in deletion requests
2. WHEN an If-Match condition fails (ETag mismatch, indicating the object was modified since listing), THE S3rm_Tool SHALL log the failure and skip the object
3. THE S3rm_Tool SHALL support enabling If-Match via a boolean command-line flag (--if-match)
4. WHEN using If-Match with batch deletions, THE Batch_Deleter SHALL include per-object ETags in the DeleteObjects request and handle conditional deletion failures appropriately

### Requirement 12: Library API Support

**User Story:** As a Rust developer, I want to use s3rm-rs as a library in my applications similar to s3sync, so that I can integrate deletion functionality programmatically.

#### Acceptance Criteria

1. THE s3rm library SHALL implement all core deletion functionality independent of the CLI
2. THE S3rm_Tool CLI SHALL be built entirely on top of the s3rm library without direct AWS SDK calls
3. THE library API SHALL expose all features available in the CLI for programmatic access
4. THE library API SHALL provide functions for configuring deletion operations (filters, workers, batch size)
5. THE library API SHALL support registering Rust filter callbacks (Filter_Callback_Rust) for custom filtering logic
6. THE library API SHALL support registering Rust event callbacks (Event_Callback_Rust) to receive deletion events (progress, errors, completions)
7. THE library API SHALL support registering Lua callbacks via script paths for filter and event operations
8. THE library API SHALL provide async functions that return results for error handling
9. THE library API SHALL be documented with rustdoc comments and usage examples

### Requirement 13: CI/CD Integration

**User Story:** As a DevOps engineer, I want the tool to work seamlessly in CI/CD pipelines, so that I can automate deletion operations.

#### Acceptance Criteria

1. WHEN running in non-interactive environments (no TTY) without the --force flag, THE S3rm_Tool SHALL return an error (exit code 2) to prevent unsafe unconfirmed deletions. WHERE the --force flag or --dry-run flag is provided, THE S3rm_Tool SHALL proceed without prompting. WHERE JSON logging is enabled without --force, THE S3rm_Tool SHALL also return an error since interactive prompts would corrupt structured output
2. WHERE the force flag is provided, THE S3rm_Tool SHALL execute deletions without requiring user confirmation
3. WHERE JSON logging is enabled, THE S3rm_Tool SHALL output machine-readable JSON logs for integration with log aggregation systems
4. THE S3rm_Tool SHALL return distinct exit codes for different failure scenarios (authentication, network, partial failure)
5. THE S3rm_Tool SHALL support reading credentials from environment variables for CI/CD environments
6. THE S3rm_Tool SHALL output all log messages (including errors) to stdout via tracing-subscriber by default
7. WHERE color output is not explicitly disabled, THE S3rm_Tool SHALL use colored output. Color can be disabled via the --disable-color-tracing flag or DISABLE_COLOR_TRACING environment variable
