# s3rm-rs Marketing Memory

## Project Facts (Verified Against Codebase)
- **Filter count**: 13 CLI filter options + Rust callback API (14 total filter mechanisms)
- **Express One Zone**: Detected by `--x-s3` bucket name suffix; parallel listing disabled by default
- **Worker range**: 1-65,535 (u16)
- **Batch size range**: 1-1,000
- **Default config**: worker-size 24, batch-size 200, max-parallel-listings 16
- **Property test count**: 47 correctness properties (proptest)

## Competitor Research (Feb 2026)
- **s5cmd**: DOES support batch deletion (DeleteObjects API, up to 1000/req) and dry-run. Does NOT have: confirmation prompts, max-delete, regex (glob only), versioning, If-Match, library API, Lua.
- **rclone**: Does NOT use batch DeleteObjects API (open issue #8160 since Nov 2024). Supports: glob + Go regex, size/time filters, dry-run, interactive mode (-i). No max-delete, no If-Match, no library API.
- **AWS CLI s3 rm**: Single-threaded, one-at-a-time deletion. No batch API, no filtering, no safety features.

## AWS S3 Pricing (Verified Feb 2026)
- DELETE requests: FREE (confirmed on aws.amazon.com/s3/pricing)
- LIST requests: $0.005 per 1,000 requests
- Cost savings come from reducing storage costs by deleting unneeded objects, not from DELETE request fees

## README Structure
- Comparison table now includes rclone column
- "Flexibility" and "S3 Express One Zone support" sections added after "Easy to use"
- "AI-generated software" section added before Acknowledgments
- Table of contents updated with new sections
