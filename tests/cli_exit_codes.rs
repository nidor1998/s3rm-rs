//! Integration tests that spawn the real `s3rm` binary to exercise the
//! process-level exit-code paths in `src/bin/s3rm/main.rs`.
//!
//! Unlike the `e2e_*` suites, these tests never touch AWS: every scenario
//! errors out during argument parsing or the pre-deletion safety check, before
//! any S3 request is made. They are therefore safe to run in normal
//! `cargo test` runs and cover the `run()` / `load_config_exit_if_err()` exit
//! branches that unit tests cannot reach (those call `std::process::exit`).

use std::process::{Command, Stdio};

/// Path to the compiled `s3rm` binary, provided by Cargo for integration tests.
fn s3rm_bin() -> &'static str {
    env!("CARGO_BIN_EXE_s3rm")
}

/// Base command with stdin/stdout/stderr detached from any TTY so the safety
/// check treats the environment as non-interactive.
fn base_command() -> Command {
    let mut cmd = Command::new(s3rm_bin());
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Avoid reading any real AWS configuration from the developer's machine.
    cmd.arg("--aws-config-file")
        .arg("./test_data/test_config/config")
        .arg("--aws-shared-credentials-file")
        .arg("./test_data/test_config/credentials")
        .arg("--target-access-key")
        .arg("dummy")
        .arg("--target-secret-access-key")
        .arg("dummy");
    cmd
}

/// An `s3://` target with an empty bucket passes the clap value-parser (it
/// starts with `s3://` and is longer than five characters) but is rejected by
/// `parse_target()` while building the `Config`. `load_config_exit_if_err()`
/// then turns that into a clap `ValueValidation` error and exits with code 2.
#[test]
fn empty_bucket_target_exits_with_config_error() {
    let output = base_command()
        .arg("s3:///prefix")
        .output()
        .expect("failed to spawn s3rm");

    assert_eq!(
        output.status.code(),
        Some(2),
        "empty-bucket target should exit with code 2; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

const INVALID_TARGET_MESSAGE: &str = "target must be an S3 path (e.g. s3://bucket/prefix)";

/// `?` or `#` directly after the bucket name is not a prefix and is not valid
/// in a bucket name, so the clap value-parser rejects the target and the
/// process exits with code 2 before any S3 request is made.
#[test]
fn query_or_fragment_after_bucket_exits_with_argument_error() {
    for target in [
        "s3://test-bucket?old",
        "s3://test-bucket#old",
        "s3://test-bucket?old/prefix/",
        "s3://test-bucket#old/prefix/",
    ] {
        let output = base_command()
            .arg("--force")
            .arg(target)
            .output()
            .expect("failed to spawn s3rm");
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(
            output.status.code(),
            Some(2),
            "target {target} should exit with code 2; stderr: {stderr}"
        );
        assert!(
            stderr.contains(INVALID_TARGET_MESSAGE),
            "target {target} should report an invalid target; stderr: {stderr}"
        );
    }
}

/// `?`, `#`, percent escapes and dot segments after the first `/` are part of
/// the prefix, so such a target passes argument parsing. With an unreachable
/// endpoint the run then fails while listing, not with an invalid-target error.
#[test]
fn query_or_fragment_in_prefix_is_accepted_as_target() {
    for target in [
        "s3://test-bucket/data#old/",
        "s3://test-bucket/data?old/",
        "s3://test-bucket/a%20b/../c#d?e",
    ] {
        let output = base_command()
            .arg("--dry-run")
            .arg("--target-endpoint-url")
            .arg("https://anything.invalid")
            .arg("--connect-timeout-milliseconds")
            .arg("1")
            .arg("--aws-max-attempts")
            .arg("0")
            .arg(target)
            .output()
            .expect("failed to spawn s3rm");
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            !stderr.contains(INVALID_TARGET_MESSAGE),
            "target {target} should pass argument parsing; stderr: {stderr}"
        );
        assert_ne!(
            output.status.code(),
            Some(101),
            "target {target} should not cause an abnormal termination; stderr: {stderr}"
        );
    }
}

/// The help text of every size option says that a lowercase `b` means bits.
#[test]
fn help_documents_case_sensitive_size_suffixes() {
    let output = Command::new(s3rm_bin())
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to spawn s3rm");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));

    let note = "A suffix ending in a lowercase b means bits, not bytes (e.g. 8Mb = 1MB).";
    let expected_count = if stdout.contains("--lua-vm-memory-limit") {
        3
    } else {
        2
    };
    assert_eq!(
        stdout.matches(note).count(),
        expected_count,
        "--help output: {stdout}"
    );
}

/// Running a destructive deletion in a non-interactive environment (no TTY)
/// without `--force` and without `--dry-run` must fail the safety check. This
/// drives `run()` -> `check_prerequisites()` -> the non-cancelled error branch,
/// which exits with the `InvalidConfig` exit code (2) without contacting S3.
#[test]
fn non_interactive_without_force_exits_with_config_error() {
    let output = base_command()
        .arg("s3://test-bucket/prefix/")
        .output()
        .expect("failed to spawn s3rm");

    assert_eq!(
        output.status.code(),
        Some(2),
        "non-interactive run without --force should exit with code 2; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--force"),
        "error should mention the --force requirement; stderr: {stderr}"
    );
}

/// `--dry-run` skips the confirmation prompt, so a non-interactive environment
/// is allowed. With an unreachable endpoint the listing phase fails and the
/// process exits non-zero via `run()`'s error-handling branch — but never with
/// the abnormal-termination code (101), which is reserved for panics.
#[test]
fn dry_run_unreachable_endpoint_exits_nonzero_without_panic() {
    let output = base_command()
        .arg("--dry-run")
        .arg("--target-endpoint-url")
        .arg("https://anything.invalid")
        .arg("--connect-timeout-milliseconds")
        .arg("1")
        .arg("--aws-max-attempts")
        .arg("0")
        .arg("s3://test-bucket/prefix/")
        .output()
        .expect("failed to spawn s3rm");

    let code = output.status.code();
    assert_ne!(
        code,
        Some(0),
        "unreachable endpoint should not succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(
        code,
        Some(101),
        "listing failure should be a normal error, not an abnormal termination"
    );
}
