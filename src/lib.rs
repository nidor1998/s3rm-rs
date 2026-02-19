/*!
# Overview
s3rm-rs is an extremely fast Amazon S3 object deletion tool.
It can be used to delete objects from S3 buckets with powerful filtering,
safety features, and versioning support.

## Features
- **High Performance**: Parallel deletion using S3 batch API (up to 1000 objects per request)
- **Flexible Filtering**: Regex, size, time, and Lua script-based filtering
- **Safety First**: Dry-run mode, confirmation prompts, force flag, max-delete threshold
- **Versioning Support**: Delete markers, all-versions deletion
- **Library-First**: All CLI features available as a Rust library
- **s3sync Compatible**: Reuses s3sync's proven infrastructure (~90% code reuse)

## As a Library
s3rm-rs can be used as a Rust library.
The s3rm CLI is a thin wrapper over the s3rm-rs library.
All CLI features are available in the library.

Example usage
=============

```toml
[dependencies]
s3rm-rs = "0.1"
tokio = { version = "1", features = ["full"] }
```

```no_run
// use s3rm_rs::config::Config;
// use s3rm_rs::config::args::parse_from_args;
// use s3rm_rs::pipeline::DeletionPipeline;
// use s3rm_rs::types::token::create_pipeline_cancellation_token;
//
// #[tokio::main]
// async fn main() {
//     let args = vec![
//         "s3rm",
//         "s3://my-bucket/prefix/",
//         "--dry-run",
//         "--force",
//     ];
//
//     let parsed_args = parse_from_args(args).unwrap();
//     let config = Config::try_from(parsed_args).unwrap();
//     let cancellation_token = create_pipeline_cancellation_token();
//     let mut pipeline = DeletionPipeline::new(config, cancellation_token).await;
//     pipeline.close_stats_sender();
//     pipeline.run().await;
//
//     if pipeline.has_error() {
//         eprintln!("{:?}", pipeline.get_errors_and_consume().unwrap()[0]);
//     }
// }
```
*/

#![allow(clippy::collapsible_if)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::unnecessary_unwrap)]

pub mod config;
pub mod filters;
pub mod lister;
pub mod stage;
pub mod storage;
pub mod types;

#[cfg(test)]
mod tests {
    #[test]
    fn library_crate_loads() {
        // Basic sanity check that the library crate compiles and loads
        assert!(true);
    }
}
