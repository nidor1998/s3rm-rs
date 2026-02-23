# Product Overview

s3rm-rs is an Amazon S3 object deletion tool designed for bulk deletion operations. Built as a sibling to s3sync, it leverages Rust's performance and the AWS SDK to provide reliable deletion of S3 objects with comprehensive safety features and versioning support.

## Key Features

- Bulk deletion up to ~3,500 objects/second (the S3 API limit)
- Batch deletion using S3's DeleteObjects API (up to 1000 objects per request)
- Parallel processing with configurable worker pools (1-65,535 workers)
- Comprehensive filtering: regex patterns, size, time, content-type, metadata, tags, Lua callbacks
- Safety features: dry-run mode, confirmation prompts, deletion limits
- S3 versioning support: delete markers, all-versions deletion
- Library-first architecture: full programmatic API access
- Cross-platform support: Linux (glibc/musl), Windows, macOS (x86_64/ARM64)

## Use Cases

- Data lifecycle management and automated retention policies
- Bucket cleanup and storage cost optimization
- Bulk object deletion (thousands to millions of objects)
- CI/CD integration for automated cleanup operations
