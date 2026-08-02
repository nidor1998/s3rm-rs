//! Process-level broken-pipe regression tests. Like `cli_exit_codes.rs`,
//! these never touch AWS: completions, help, and version never leave the
//! process, and the pipeline test fails during listing against an
//! unreachable endpoint.
//!
//! Each test hands the child a stdout (or stderr) whose read end is already
//! closed, so every write to it fails with `BrokenPipe` from the first byte —
//! the worst case of piping to a consumer that exits without reading, such as
//! `s3rm --auto-complete-shell bash | head 1` (`head` treats `1` as a file
//! name, fails to open it, and never reads its input) or `head -1`/`grep -q`/
//! a pager closed early. The binary must exit through its normal paths: never
//! panic ("failed printing to stdout: Broken pipe") and never die of SIGPIPE
//! (which would surface here as `status.code() == None`).

use std::process::{Command, Stdio};

fn s3rm() -> Command {
    Command::new(env!("CARGO_BIN_EXE_s3rm"))
}

/// Run `cmd` with a pre-closed stdout pipe and return (exit_code, stderr).
fn run_with_closed_stdout(cmd: &mut Command) -> (Option<i32>, String) {
    let (reader, writer) = std::io::pipe().expect("failed to create pipe");
    // Close the read end before the child even starts: with no readers left,
    // every stdout write in the child fails with EPIPE immediately.
    drop(reader);

    let output = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped())
        .output()
        .expect("failed to spawn s3rm binary");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Run `cmd` with pre-closed stdout AND stderr pipes; only the exit code is
/// observable. This is the `s3rm ... 2>&1 | head 1` case, where both streams
/// share one pipe whose reader is already gone.
fn run_with_closed_stdout_and_stderr(cmd: &mut Command) -> Option<i32> {
    let (out_reader, out_writer) = std::io::pipe().expect("failed to create pipe");
    let (err_reader, err_writer) = std::io::pipe().expect("failed to create pipe");
    drop(out_reader);
    drop(err_reader);

    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(out_writer))
        .stderr(Stdio::from(err_writer))
        .status()
        .expect("failed to spawn s3rm binary")
        .code()
}

fn assert_exits_zero_without_panic(code: Option<i32>, stderr: &str, what: &str) {
    assert!(
        !stderr.contains("panicked"),
        "{what} must not panic on a closed stdout pipe; stderr: {stderr}"
    );
    assert_eq!(
        code,
        Some(0),
        "{what} must exit 0 on a closed stdout pipe (None = killed by \
         SIGPIPE); stderr: {stderr}"
    );
}

/// Completion scripts are rendered to a buffer and written pipe-safely —
/// clap_complete itself would panic ("failed to write completion file") if
/// its generator wrote straight into a closed stdout.
#[test]
fn completion_script_with_closed_stdout_exits_zero() {
    for shell in ["bash", "zsh", "fish"] {
        let (code, stderr) = run_with_closed_stdout(s3rm().args(["--auto-complete-shell", shell]));
        assert_exits_zero_without_panic(code, &stderr, &format!("--auto-complete-shell {shell}"));
    }
}

/// `--help` and `--version` are printed by clap, whose `Error::exit`
/// swallows write errors ("Swallow broken pipe errors" in clap itself).
/// Pinned here so a clap regression would be caught.
#[test]
fn help_and_version_with_closed_stdout_exit_zero() {
    let (code, stderr) = run_with_closed_stdout(s3rm().arg("--help"));
    assert_exits_zero_without_panic(code, &stderr, "--help");

    let (code, stderr) = run_with_closed_stdout(s3rm().arg("--version"));
    assert_exits_zero_without_panic(code, &stderr, "--version");
}

/// Config errors are reported via `clap::Error::raw(...).exit()`, which also
/// swallows a closed stderr: the exit code must stay 2 (InvalidConfig), not
/// become a panic or SIGPIPE death.
#[test]
fn config_error_with_closed_stderr_still_exits_two() {
    let code = run_with_closed_stdout_and_stderr(s3rm().arg("s3:///prefix"));
    assert_eq!(
        code,
        Some(2),
        "an invalid target with closed output pipes must still exit 2 \
         (None = killed by SIGPIPE, 101 = panicked)"
    );
}

/// The final deletion summary writes a spacing line to stderr after the
/// indicatif message. With stderr a closed pipe that `eprintln!()` panicked
/// inside the indicator task, and `main` turned the join error into exit 101
/// (abnormal termination) — even though the pipeline itself had already
/// finished. Now the summary lines are best-effort: the process must exit
/// through the normal error path (exit 1, from the listing failure against
/// the unreachable endpoint), like `cli_exit_codes.rs` pins for open pipes.
#[test]
fn deletion_summary_with_closed_stderr_exits_through_normal_error_path() {
    let code = run_with_closed_stdout_and_stderr(
        s3rm()
            .arg("--dry-run")
            .arg("--aws-config-file")
            .arg("./test_data/test_config/config")
            .arg("--aws-shared-credentials-file")
            .arg("./test_data/test_config/credentials")
            .arg("--target-access-key")
            .arg("dummy")
            .arg("--target-secret-access-key")
            .arg("dummy")
            .arg("--target-endpoint-url")
            .arg("https://anything.invalid")
            .arg("--connect-timeout-milliseconds")
            .arg("1")
            .arg("--aws-max-attempts")
            .arg("0")
            .arg("s3://test-bucket/prefix/"),
    );
    assert_eq!(
        code,
        Some(1),
        "a listing failure with closed output pipes must exit 1 \
         (101 = the summary line panicked, None = killed by SIGPIPE)"
    );
}
