//! Pipe-safe stdout printing.
//!
//! Stdout writes panic (or, inside clap_complete, `panic!` explicitly) when
//! stdout is a pipe whose reader has already gone away — piping to a
//! `head`/`grep -q`/pager that exited before the output ended, or
//! `s3rm --auto-complete-shell bash | head 1`, where `head` fails to open
//! the file `1` and never reads its input at all. The only stdout output
//! s3rm produces is the shell-completion script, and it goes through this
//! helper instead: `BrokenPipe` is swallowed — a reader that stops early is
//! a normal way for a pipeline to end — so the command still exits 0. Any
//! other write error (full disk or I/O error on a redirect) is propagated
//! so the command fails loudly instead of silently dropping output.
//!
//! The stderr counterpart for tracing output is
//! `tracing_init::PipeSafeWriter`. The confirmation prompt intentionally
//! keeps its strict `println!` writes: it is only reachable when stdout is
//! a TTY (`SafetyChecker` errors out in non-interactive environments), and
//! a TTY cannot produce `BrokenPipe`.

use std::io::{ErrorKind, Write};

/// Write `bytes` to stdout as-is (no trailing newline), swallowing
/// BrokenPipe. Used for pre-rendered output such as shell-completion
/// scripts.
pub fn write_all_pipe_safe(bytes: &[u8]) -> std::io::Result<()> {
    write_all_ignoring_broken_pipe(&mut std::io::stdout().lock(), bytes)
}

fn write_all_ignoring_broken_pipe(writer: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    ignore_broken_pipe(writer.write_all(bytes).and_then(|()| writer.flush()))
}

fn ignore_broken_pipe(result: std::io::Result<()>) -> std::io::Result<()> {
    match result {
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails writes (or, with `fail_flush`, only the flush) with `kind`.
    /// The real closed-pipe scenario is exercised process-level by
    /// `tests/cli_broken_pipe.rs`.
    struct FailWriter {
        kind: ErrorKind,
        fail_flush: bool,
    }

    impl Write for FailWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.fail_flush {
                Ok(buf.len())
            } else {
                Err(std::io::Error::new(self.kind, "simulated write failure"))
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                Err(std::io::Error::new(self.kind, "simulated flush failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn write_all_passes_bytes_through_verbatim() {
        let mut buf = Vec::new();
        write_all_ignoring_broken_pipe(&mut buf, b"complete -c s3rm\n").unwrap();
        assert_eq!(buf, b"complete -c s3rm\n");
    }

    #[test]
    fn write_all_swallows_broken_pipe_on_write() {
        let mut writer = FailWriter {
            kind: ErrorKind::BrokenPipe,
            fail_flush: false,
        };
        write_all_ignoring_broken_pipe(&mut writer, b"data").unwrap();
    }

    #[test]
    fn write_all_swallows_broken_pipe_on_flush() {
        let mut writer = FailWriter {
            kind: ErrorKind::BrokenPipe,
            fail_flush: true,
        };
        write_all_ignoring_broken_pipe(&mut writer, b"data").unwrap();
    }

    #[test]
    fn write_all_propagates_other_errors() {
        // A failed redirect (disk full, I/O error) must still fail the
        // command — only a vanished reader is benign.
        let mut writer = FailWriter {
            kind: ErrorKind::StorageFull,
            fail_flush: false,
        };
        let err = write_all_ignoring_broken_pipe(&mut writer, b"data").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::StorageFull);
    }

    #[test]
    fn write_all_propagates_other_errors_on_flush() {
        let mut writer = FailWriter {
            kind: ErrorKind::StorageFull,
            fail_flush: true,
        };
        let err = write_all_ignoring_broken_pipe(&mut writer, b"data").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::StorageFull);
    }
}
