# s3rm

**Extremely fast Amazon S3 object deletion tool**

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.91.0%2B-orange.svg)](https://www.rust-lang.org/)

Delete thousands to millions of S3 objects at speeds approaching Amazon S3's own throughput limit of **~3,500 objects/second**. Built in Rust for maximum performance and safety.

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Requirements](#requirements)
- [License](#license)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Usage Examples](#usage-examples)
- [Filtering](#filtering)
- [Safety Features](#safety-features)
- [Versioning Support](#versioning-support)
- [Library API](#library-api)
- [Performance Tuning](#performance-tuning)
- [CI/CD Integration](#cicd-integration)
- [S3-Compatible Services](#s3-compatible-services)
- [All CLI Options](#all-cli-options)
- [Exit Codes](#exit-codes)
- [Lua Scripting](#lua-scripting)
- [Acknowledgments](#acknowledgments)

---

## Overview

s3rm is a purpose-built tool for high-performance bulk deletion of Amazon S3 objects. Whether you're cleaning up terabytes of old logs, enforcing data retention policies, or purging versioned buckets, s3rm handles it with a streaming pipeline that keeps memory usage low and throughput high.

Built as a sibling to [s3sync](https://github.com/nidor1998/s3sync), s3rm shares the same battle-tested AWS SDK integration, retry logic, and filtering engine. While s3sync moves data between S3 buckets, s3rm focuses exclusively on deletion — and does it extremely well.

### How It Works

```
ObjectLister → [Filters] → ObjectDeleter Workers → Terminator
```

Objects stream through the pipeline one stage at a time. The lister fetches keys from S3 using parallel pagination, filters narrow down which objects to delete, and a pool of concurrent workers executes batch deletions against the S3 API. Nothing is loaded into memory all at once.

---

## Features

### High Performance
- **Batch deletion** using S3's `DeleteObjects` API — up to 1,000 objects per request
- **Parallel workers** — up to 65,535 concurrent deletion workers (default: 24)
- **Parallel listing** — concurrent `ListObjectsV2` pagination for faster enumeration
- **Streaming pipeline** — constant memory usage regardless of the number of objects
- **Rate limiting** — configurable objects-per-second cap to stay within your budget

### Powerful Filtering
- **Regex patterns** — filter by key, content type, user-defined metadata, or tags
- **Size filters** — delete objects smaller or larger than a threshold
- **Time filters** — delete objects modified before or after a given timestamp
- **Lua callbacks** — write custom filter logic in Lua scripts
- **Rust callbacks** — register programmatic filter functions via the library API
- **Combinable** — all filters are combined with logical AND

### Safety First
- **Dry-run mode** — preview exactly what would be deleted before doing it
- **Confirmation prompt** — requires explicit "yes" before destructive operations
- **Max-delete threshold** — automatically stops after deleting a set number of objects
- **Force flag** — skip prompts for scripted or CI/CD use
- **Non-TTY detection** — automatically skips interactive prompts in pipelines

### S3 Versioning
- **Delete markers** — creates standard delete markers by default
- **All-versions deletion** — remove every version of matching objects, including delete markers
- **Version-aware dry-run** — counts each version as a separate object in statistics

### Optimistic Locking
- **ETag-based conditional deletion** — use `--if-match` to prevent deleting objects that were modified after listing
- **Race condition protection** — skips objects whose ETag has changed since enumeration

### Observability
- **Progress bar** — real-time objects deleted, bytes reclaimed, and deletion rate
- **Configurable verbosity** — from silent (`-qq`) to debug (`-vvv`)
- **JSON logging** — structured logs for integration with log aggregation systems
- **Event callbacks** — receive real-time events (Lua scripts or Rust callbacks)

### Developer-Friendly
- **Library-first** — all functionality is available as a Rust library crate
- **Async API** — built on Tokio for easy integration into async applications
- **Extensible** — register custom filter and event callbacks programmatically
- **Shell completions** — auto-generate completions for Bash, Zsh, Fish, PowerShell

---

## Requirements

- AWS credentials configured via any standard method:
  - Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
  - AWS credentials file (`~/.aws/credentials`)
  - AWS config file (`~/.aws/config`) with profiles
  - IAM instance roles (EC2, ECS, Lambda)
  - SSO/federated authentication
- Rust 1.91.0+ (for building from source)

---

## License

Apache License 2.0. See [LICENSE](LICENSE) for details.

---

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/nidor1998/s3rm-rs.git
cd s3rm-rs

# Build release binary
cargo build --release

# The binary is at ./target/release/s3rm
```

### With Lua Scripting Support

```bash
cargo build --release --features lua_support
```

---

## Quick Start

```bash
# Preview what would be deleted (dry-run)
s3rm s3://my-bucket/old-logs/ --dry-run

# Delete all objects under a prefix
s3rm s3://my-bucket/old-logs/

# Delete with force (skip confirmation)
s3rm s3://my-bucket/temp/ --force

# Delete matching a regex pattern
s3rm s3://my-bucket/data/ --filter-include-regex '.*\.tmp$' --force

# Delete all versions in a versioned bucket
s3rm s3://my-bucket/archive/ --delete-all-versions --force
```

---

## Usage Examples

### Delete by Prefix

The simplest usage — delete all objects under a given S3 prefix:

```bash
s3rm s3://my-bucket/logs/2023/
```

You'll be asked to confirm before any objects are deleted.

### Dry-Run Mode

Preview exactly what would happen without deleting anything:

```bash
s3rm s3://my-bucket/logs/2023/ --dry-run
```

Each object that would be deleted is logged at info level with a `[dry-run]` prefix, and summary statistics are displayed at the end.

### Delete with Regex Filter

Delete only `.tmp` files:

```bash
s3rm s3://my-bucket/data/ --filter-include-regex '.*\.tmp$' --force
```

Exclude certain files from deletion:

```bash
s3rm s3://my-bucket/data/ --filter-exclude-regex '.*\.keep$' --force
```

### Delete by Size

Delete objects smaller than 1 KB (likely empty or corrupt):

```bash
s3rm s3://my-bucket/uploads/ --filter-smaller-size 1KB --force
```

Delete objects larger than 1 GB:

```bash
s3rm s3://my-bucket/backups/ --filter-larger-size 1GiB --force
```

### Delete by Modified Time

Delete objects older than a specific date:

```bash
s3rm s3://my-bucket/logs/ --filter-mtime-before 2023-01-01T00:00:00Z --force
```

Delete objects modified after a specific date:

```bash
s3rm s3://my-bucket/temp/ --filter-mtime-after 2024-06-01T00:00:00Z --force
```

### Combined Filters

Filters combine with logical AND. Delete `.log` files older than 90 days and smaller than 10 MB:

```bash
s3rm s3://my-bucket/logs/ \
  --filter-include-regex '.*\.log$' \
  --filter-mtime-before 2024-09-01T00:00:00Z \
  --filter-smaller-size 10MiB \
  --force
```

### Delete All Versions

On versioned buckets, delete every version of every object under a prefix:

```bash
s3rm s3://my-bucket/old-data/ --delete-all-versions --force
```

### Set a Deletion Limit

Stop after deleting 1,000 objects (safety net for large buckets):

```bash
s3rm s3://my-bucket/data/ --max-delete 1000 --force
```

### Verbose Output

Increase verbosity for operational monitoring:

```bash
# Standard operational logs
s3rm s3://my-bucket/logs/ -v --force

# Detailed logs (timestamps, batch info, retries)
s3rm s3://my-bucket/logs/ -vv --force

# Debug-level logs (API requests, internal state)
s3rm s3://my-bucket/logs/ -vvv --force
```

---

## Filtering

s3rm provides a comprehensive filtering system. All filters are combined with logical AND — an object must pass every active filter to be deleted.

### Key Regex Filters

```bash
--filter-include-regex PATTERN   # Delete only objects matching this regex
--filter-exclude-regex PATTERN   # Skip objects matching this regex
```

### Content Type Regex Filters

```bash
--filter-include-content-type-regex PATTERN
--filter-exclude-content-type-regex PATTERN
```

### Metadata Regex Filters

Filter by user-defined metadata. Keys must be sorted alphabetically and separated by commas:

```bash
--filter-include-metadata-regex "key1=(value1|value2),key2=value2"
--filter-exclude-metadata-regex "key1=value1"
```

> Note: Metadata filters may require an extra API call per object.

### Tag Regex Filters

Filter by S3 object tags. Keys must be sorted alphabetically and separated by `&`:

```bash
--filter-include-tag-regex "env=(dev|staging)&team=data"
--filter-exclude-tag-regex "retain=true"
```

> Note: Tag filters require an extra API call per object.

### Size Filters

```bash
--filter-smaller-size SIZE   # Delete objects smaller than SIZE
--filter-larger-size SIZE    # Delete objects larger than or equal to SIZE
```

Supported suffixes: `KB`, `KiB`, `MB`, `MiB`, `GB`, `GiB`, `TB`, `TiB`

### Time Filters

```bash
--filter-mtime-before TIMESTAMP   # Delete objects modified before this time
--filter-mtime-after TIMESTAMP    # Delete objects modified at or after this time
```

Timestamps use RFC 3339 format: `2024-01-15T00:00:00Z`

---

## Safety Features

### Confirmation Prompt

By default, s3rm shows the target path and asks for confirmation before deleting:

```
You are about to delete objects from: s3://my-bucket/important-data/
Type "yes" to continue:
```

Only the full word "yes" is accepted. Abbreviated responses like "y" are rejected.

### Dry-Run Mode

Use `--dry-run` (or `-d`) to run the full pipeline without actually deleting anything:

```bash
s3rm s3://my-bucket/data/ --dry-run
```

Dry-run mode:
- Lists and filters objects normally
- Logs each object that *would* be deleted with a `[dry-run]` prefix
- Displays summary statistics (object count, total size)
- Forces verbosity to at least info level so you always see results

### Max-Delete Threshold

Set a hard limit on how many objects can be deleted in a single run:

```bash
s3rm s3://my-bucket/data/ --max-delete 500 --force
```

The pipeline cancels gracefully once the threshold is reached.

### Force Flag

Skip the confirmation prompt for automated use:

```bash
s3rm s3://my-bucket/data/ --force
```

---

## Versioning Support

S3 versioned buckets store multiple versions of each object. s3rm handles both scenarios:

### Default Behavior (Delete Markers)

Without `--delete-all-versions`, deleting from a versioned bucket creates delete markers — the objects appear deleted but previous versions are preserved:

```bash
s3rm s3://my-bucket/data/
```

### Delete All Versions

With `--delete-all-versions`, every version of every matching object is permanently removed, including delete markers:

```bash
s3rm s3://my-bucket/data/ --delete-all-versions --force
```

---

## Library API

s3rm is designed library-first. All CLI functionality is available programmatically through the `s3rm_rs` crate.

### Basic Usage

```rust
use s3rm_rs::config::Config;
use s3rm_rs::pipeline::DeletionPipeline;
use s3rm_rs::types::token::create_pipeline_cancellation_token;

#[tokio::main]
async fn main() {
    let config = Config::try_from(cli_args).unwrap();
    let cancellation_token = create_pipeline_cancellation_token();
    let mut pipeline = DeletionPipeline::new(config, cancellation_token).await;
    pipeline.close_stats_sender();
    pipeline.run().await;

    if pipeline.has_error() {
        let errors = pipeline.get_errors_and_consume().unwrap();
        eprintln!("Errors: {:?}", errors);
    }
}
```

### Custom Filter Callback

Register a Rust function to filter objects programmatically:

```rust
use async_trait::async_trait;
use s3rm_rs::types::filter_callback::FilterCallback;
use s3rm_rs::types::S3Object;
use anyhow::Result;

struct MyFilter;

#[async_trait]
impl FilterCallback for MyFilter {
    async fn filter(&mut self, object: &S3Object) -> Result<bool> {
        // Return true to delete, false to skip
        Ok(object.key.ends_with(".tmp"))
    }
}
```

### Custom Event Callback

Monitor deletion events in real time:

```rust
use async_trait::async_trait;
use s3rm_rs::types::event_callback::{EventCallback, EventData};

struct MyEventHandler;

#[async_trait]
impl EventCallback for MyEventHandler {
    async fn on_event(&mut self, event: EventData) {
        println!("Event: {:?}, Key: {:?}", event.event_type, event.key);
    }
}
```

### Event Types

| Event Type | Description |
|---|---|
| `PIPELINE_START` | Pipeline execution has started |
| `PIPELINE_END` | Pipeline execution has completed |
| `DELETE_COMPLETE` | An object was successfully deleted |
| `DELETE_FAILED` | An object deletion failed |
| `DELETE_FILTERED` | An object was filtered out (not deleted) |
| `DELETE_WARNING` | A non-fatal warning occurred |
| `PIPELINE_ERROR` | A pipeline-level error occurred |
| `DELETE_CANCEL` | A deletion was cancelled (e.g., max-delete reached) |
| `STATS_REPORT` | Periodic statistics update |

---

## Performance Tuning

### Worker Count

Control the number of concurrent deletion workers:

```bash
s3rm s3://my-bucket/data/ --worker-size 48 --force
```

Default: **24 workers**. Range: 1–65,535. The default is tuned to approach S3's ~3,500 objects/second limit.

### Batch Size

Control how many objects are grouped per `DeleteObjects` API call:

```bash
s3rm s3://my-bucket/data/ --batch-size 500 --force
```

Default: **200**. Range: 1–1,000. Set to 1 to use single-object `DeleteObject` API calls.

### Parallel Listings

Control how many concurrent listing operations are used:

```bash
s3rm s3://my-bucket/data/ --max-parallel-listings 32
```

Default: **16**. More parallel listings speed up enumeration of large prefixes.

### Rate Limiting

Cap deletion throughput to avoid throttling or to control costs:

```bash
s3rm s3://my-bucket/data/ --rate-limit-objects 1000 --force
```

Minimum: 10 objects/second.

---

## CI/CD Integration

s3rm is designed to work seamlessly in automated pipelines.

### Non-Interactive Mode

In non-TTY environments (CI/CD pipelines, cron jobs), s3rm automatically skips interactive prompts. Always use `--force` for unattended execution:

```bash
s3rm s3://my-bucket/temp/ --force
```

### JSON Logging

Enable structured JSON logs for log aggregation systems:

```bash
s3rm s3://my-bucket/temp/ --json-tracing --force
```

### Quiet Mode

Suppress progress output for cleaner CI logs:

```bash
s3rm s3://my-bucket/temp/ --show-no-progress --force
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (all objects deleted, or operation cancelled) |
| 1 | General error (AWS SDK, Lua script, I/O, pipeline failure) |
| 2 | Invalid arguments or configuration |
| 3 | Partial failure (some objects deleted, some failed) |

### Example CI Script

```bash
#!/bin/bash
set -e

# Delete temp objects older than 30 days
s3rm s3://my-bucket/temp/ \
  --filter-mtime-before "$(date -u -d '30 days ago' +%Y-%m-%dT%H:%M:%SZ)" \
  --force \
  --json-tracing \
  --show-no-progress

exit_code=$?
if [ $exit_code -eq 3 ]; then
  echo "Warning: some deletions failed"
fi
```

---

## S3-Compatible Services

s3rm works with any S3-compatible storage service (MinIO, Wasabi, Cloudflare R2, etc.):

```bash
s3rm s3://my-bucket/data/ \
  --target-endpoint-url https://minio.example.com:9000 \
  --target-force-path-style \
  --force
```

---

## All CLI Options

### General

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--dry-run` | `-d` | `false` | Preview deletions without executing them |
| `--force` | `-f` | `false` | Skip confirmation prompt |
| `--show-no-progress` | | `false` | Hide the progress bar |
| `--delete-all-versions` | | `false` | Delete all versions including delete markers |
| `--batch-size` | | `200` | Objects per batch deletion request (1–1000) |
| `--max-delete` | | | Stop after deleting this many objects |

### Filtering

| Option | Description |
|--------|-------------|
| `--filter-include-regex` | Delete only objects whose key matches this regex |
| `--filter-exclude-regex` | Skip objects whose key matches this regex |
| `--filter-include-content-type-regex` | Delete only objects whose content type matches |
| `--filter-exclude-content-type-regex` | Skip objects whose content type matches |
| `--filter-include-metadata-regex` | Delete only objects whose metadata matches |
| `--filter-exclude-metadata-regex` | Skip objects whose metadata matches |
| `--filter-include-tag-regex` | Delete only objects whose tags match |
| `--filter-exclude-tag-regex` | Skip objects whose tags match |
| `--filter-mtime-before` | Delete only objects modified before this time (RFC 3339) |
| `--filter-mtime-after` | Delete only objects modified at or after this time (RFC 3339) |
| `--filter-smaller-size` | Delete only objects smaller than this size |
| `--filter-larger-size` | Delete only objects larger than or equal to this size |

### Tracing/Logging

| Option | Default | Description |
|--------|---------|-------------|
| `-v` / `-vv` / `-vvv` | Warn | Increase verbosity level |
| `-q` / `-qq` | | Decrease verbosity (quiet / silent) |
| `--json-tracing` | `false` | Output structured JSON logs |
| `--aws-sdk-tracing` | `false` | Include AWS SDK internal traces |
| `--span-events-tracing` | `false` | Include span open/close events |
| `--disable-color-tracing` | `false` | Disable colored log output |

### AWS Configuration

| Option | Description |
|--------|-------------|
| `--aws-config-file` | Path to AWS config file |
| `--aws-shared-credentials-file` | Path to AWS shared credentials file |
| `--target-profile` | AWS CLI profile name |
| `--target-access-key` | AWS access key ID |
| `--target-secret-access-key` | AWS secret access key |
| `--target-session-token` | AWS session token |
| `--target-region` | AWS region |
| `--target-endpoint-url` | Custom S3-compatible endpoint URL |
| `--target-force-path-style` | Use path-style access (default: `false`) |
| `--target-accelerate` | Enable S3 Transfer Acceleration (default: `false`) |
| `--target-request-payer` | Enable requester-pays (default: `false`) |
| `--disable-stalled-stream-protection` | Disable stalled stream protection (default: `false`) |

### Performance

| Option | Default | Description |
|--------|---------|-------------|
| `--worker-size` | `24` | Concurrent deletion workers (1–65535) |
| `--max-parallel-listings` | `16` | Concurrent listing operations |
| `--max-parallel-listing-max-depth` | `2` | Maximum depth for parallel listings |
| `--rate-limit-objects` | | Maximum objects/second (minimum: 10) |
| `--object-listing-queue-size` | `200000` | Internal queue size for object listing |
| `--allow-parallel-listings-in-express-one-zone` | `false` | Allow parallel listings in Express One Zone storage |

### Retry Options

| Option | Default | Description |
|--------|---------|-------------|
| `--aws-max-attempts` | `10` | Maximum retry attempts for AWS SDK operations |
| `--initial-backoff-milliseconds` | `100` | Initial backoff for retries |
| `--force-retry-count` | `0` | Application-level retries after SDK retries are exhausted |
| `--force-retry-interval-milliseconds` | `1000` | Interval between application-level retries |

### Timeout Options

| Option | Description |
|--------|-------------|
| `--operation-timeout-milliseconds` | Overall operation timeout |
| `--operation-attempt-timeout-milliseconds` | Per-attempt operation timeout |
| `--connect-timeout-milliseconds` | Connection timeout |
| `--read-timeout-milliseconds` | Read timeout |

### Advanced

| Option | Default | Description |
|--------|---------|-------------|
| `--if-match` | `false` | ETag-based conditional deletion (optimistic locking) |
| `--warn-as-error` | `false` | Treat warnings as errors (exit code 1 instead of 3) |
| `--max-keys` | `1000` | Max objects per list request (1–32767) |
| `--auto-complete-shell` | | Generate shell completions (bash, zsh, fish, powershell) |

### Lua Scripting (requires `lua_support` feature)

| Option | Default | Description |
|--------|---------|-------------|
| `--filter-callback-lua-script` | | Path to Lua filter callback script |
| `--event-callback-lua-script` | | Path to Lua event callback script |
| `--allow-lua-os-library` | `false` | Allow Lua OS/IO library access |
| `--lua-vm-memory-limit` | `64MiB` | Memory limit for the Lua VM |
| `--allow-lua-unsafe-vm` | `false` | Remove all Lua sandbox restrictions |

All options can also be set via environment variables. The environment variable name matches the long option name in `SCREAMING_SNAKE_CASE` (e.g., `--worker-size` becomes `WORKER_SIZE`).

**Precedence:** CLI arguments > environment variables > defaults.

---

## Exit Codes

| Code | Name | Description |
|------|------|-------------|
| 0 | Success | All objects deleted successfully, or operation cancelled by user |
| 1 | General Error | AWS SDK error, Lua script error, I/O error, or pipeline failure |
| 2 | Invalid Config | Invalid arguments, S3 URI, or regex pattern |
| 3 | Partial Failure | Some objects deleted, some failed (use `--warn-as-error` to exit 1 instead) |

---

## Lua Scripting

> Requires building with `--features lua_support`

### Filter Callback

Write a Lua script to implement custom deletion logic. The script receives an object table and returns `true` to delete or `false` to skip:

```lua
-- filter.lua: Delete only objects larger than 1MB
function filter(object)
    return object.size > 1048576
end
```

```bash
s3rm s3://my-bucket/data/ --filter-callback-lua-script filter.lua --force
```

### Event Callback

Write a Lua script to monitor deletion events:

```lua
-- events.lua: Log each deletion
function on_event(event)
    if event.event_type == "DELETE_COMPLETE" then
        print("Deleted: " .. event.key)
    end
end
```

```bash
s3rm s3://my-bucket/data/ --event-callback-lua-script events.lua --force
```

### Sandbox

By default, Lua scripts run in safe mode with no access to the OS or I/O libraries. Use `--allow-lua-os-library` to enable file access, or `--allow-lua-unsafe-vm` to remove all sandbox restrictions (use with caution).

---

## Acknowledgments

s3rm is built on the architecture and proven patterns of [s3sync](https://github.com/nidor1998/s3sync). Approximately 90% of the codebase — including the AWS client layer, retry logic, filtering engine, Lua integration, progress reporting, and pipeline architecture — is reused from s3sync.

Built with the [AWS SDK for Rust](https://github.com/awslabs/aws-sdk-rust).
