# s3rm-rs Project Overview

## Purpose
High-performance Amazon S3 object deletion tool for bulk deletion operations. Built as a sibling to s3sync with ~90% code reuse.

## Tech Stack
- Rust 2024 edition
- Async runtime: Tokio
- AWS SDK for Rust (S3 client)
- Clap (CLI), Tracing (logging), mlua (Lua VM), Regex, Indicatif (progress bars)

## Architecture
Library-first design with streaming pipeline: List → Filter → Delete → Terminate
- Pipeline stages connected by async channels
- Configurable worker pools (1-65535 workers)
- Batch (DeleteObjects API) and single (DeleteObject API) deletion modes

## Key Modules
- `src/pipeline.rs` - DeletionPipeline orchestrator
- `src/config/` - Configuration and CLI args
- `src/storage/` - S3 storage trait and implementation
- `src/lister.rs` - Object listing with parallel pagination
- `src/filters/` - Filter stages (regex, size, time, Lua)
- `src/deleter/` - Deletion components (batch, single, worker)
- `src/callback/` - Event and filter callback managers
- `src/safety/` - Safety features (confirmation, dry-run)
- `src/lua/` - Lua VM integration
- `src/types/` - Core data types and error types
- `src/bin/s3rm/` - CLI binary entry point
