//! Process-level regression tests for SIGINT (Ctrl+C) exit-code handling.
//!
//! `s3rm` catches Ctrl+C, cancels the deletion pipeline, and must then exit
//! gracefully with code 130 (128 + SIGINT), the conventional shell encoding
//! for a run interrupted by the user. These tests run the real binary
//! against a minimal in-process S3 endpoint (no AWS access needed): the
//! endpoint keeps returning truncated list pages so the deletion runs until
//! the test sends SIGINT, and answers batch (`DeleteObjects`) and
//! single-object (`DeleteObject`) delete requests so the deletion stages
//! stay active as well.
//!
//! Covers `src/bin/s3rm/main.rs` (`is_ctrl_c_received` → exit
//! `SIGINT_EXIT_CODE`) and `src/bin/s3rm/ctrl_c_handler/mod.rs`.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Which listing API the fake endpoint emulates.
#[derive(Clone, Copy)]
enum ListingKind {
    Objects,
    #[cfg(target_family = "unix")]
    Versions,
}

/// Handle to a fake S3 endpoint running on a background thread.
struct FakeS3 {
    endpoint: String,
    pages_served: Arc<AtomicUsize>,
    deletes_served: Arc<AtomicUsize>,
    deleted_keys: Arc<Mutex<Vec<String>>>,
}

/// Serve canned S3 responses over plain HTTP/1.1, one request per
/// connection, serialized by the accept loop:
///
/// - `GET ?versioning` → versioning enabled (for `--delete-all-versions`)
/// - `GET` (list) → one page with one object, advancing its continuation
///   token / key marker each time. With `total_pages = Some(n)` the n-th
///   page is final (`IsTruncated` false); with `None` the listing is
///   endless, so the child only stops when it is signalled.
/// - `POST ?delete` (batch `DeleteObjects`) → echo every requested key
///   (and version id) back as `<Deleted>`
/// - `DELETE` (single `DeleteObject`) → 204 No Content
fn spawn_fake_s3(kind: ListingKind, total_pages: Option<usize>) -> FakeS3 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind fake S3 listener");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let pages_served = Arc::new(AtomicUsize::new(0));
    let deletes_served = Arc::new(AtomicUsize::new(0));
    let deleted_keys = Arc::new(Mutex::new(Vec::new()));

    let pages = Arc::clone(&pages_served);
    let deletes = Arc::clone(&deletes_served);
    let keys = Arc::clone(&deleted_keys);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let Some((request_line, request_body)) = read_request(&mut stream) else {
                continue;
            };

            let (status_line, body) = route_request(
                &request_line,
                &request_body,
                kind,
                total_pages,
                &pages,
                &deletes,
                &keys,
            );
            let response = format!(
                "HTTP/1.1 {status_line}\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    FakeS3 {
        endpoint,
        pages_served,
        deletes_served,
        deleted_keys,
    }
}

/// Read one HTTP request (head plus `content-length` body) and return its
/// request line and body.
fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let header_end = loop {
        match stream.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                data.extend_from_slice(&buf[..n]);
                if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
            }
            Err(_) => return None,
        }
    };

    let head = String::from_utf8_lossy(&data[..header_end]).to_string();
    let request_line = head.lines().next().unwrap_or("").to_string();

    let mut content_length = 0usize;
    for line in head.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    while data.len() - header_end < content_length {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }

    let body = String::from_utf8_lossy(&data[header_end..]).to_string();
    Some((request_line, body))
}

fn route_request(
    request_line: &str,
    request_body: &str,
    kind: ListingKind,
    total_pages: Option<usize>,
    pages_served: &AtomicUsize,
    deletes_served: &AtomicUsize,
    deleted_keys: &Mutex<Vec<String>>,
) -> (&'static str, String) {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    match method {
        // GetBucketVersioning, used by the --delete-all-versions
        // prerequisite check. Must not be confused with the `versions`
        // (ListObjectVersions) query parameter.
        "GET" if has_query_param(target, "versioning") => ("200 OK", versioning_enabled_page()),
        "GET" => {
            let page = pages_served.fetch_add(1, Ordering::SeqCst);
            let truncated = total_pages.is_none_or(|total| page + 1 < total);
            let body = match kind {
                ListingKind::Objects => objects_page(page, truncated),
                #[cfg(target_family = "unix")]
                ListingKind::Versions => versions_page(page, truncated),
            };
            ("200 OK", body)
        }
        // Batch DeleteObjects: acknowledge every requested key as deleted.
        "POST" if has_query_param(target, "delete") => {
            let requested = parse_delete_request(request_body);
            {
                let mut recorded = deleted_keys.lock().unwrap();
                recorded.extend(requested.iter().map(|(key, _)| key.clone()));
            }
            deletes_served.fetch_add(1, Ordering::SeqCst);
            ("200 OK", delete_result_page(&requested))
        }
        // Single-object DeleteObject.
        "DELETE" => {
            let path = target.split('?').next().unwrap_or("");
            let key = path.rsplit('/').next().unwrap_or("").to_string();
            deleted_keys.lock().unwrap().push(key);
            deletes_served.fetch_add(1, Ordering::SeqCst);
            ("204 No Content", String::new())
        }
        _ => ("400 Bad Request", String::new()),
    }
}

/// Whether the request target carries the query parameter `name`
/// (`?versioning` matches `versioning` but not `versions`).
fn has_query_param(target: &str, name: &str) -> bool {
    target
        .split_once('?')
        .map(|(_, query)| {
            query
                .split('&')
                .any(|param| param.split('=').next() == Some(name))
        })
        .unwrap_or(false)
}

/// Extract the `(key, version_id)` pairs from a `DeleteObjects` request body.
fn parse_delete_request(body: &str) -> Vec<(String, Option<String>)> {
    body.split("<Object>")
        .skip(1)
        .map(|chunk| {
            let object = chunk.split("</Object>").next().unwrap_or(chunk);
            (
                extract_tag(object, "Key").unwrap_or_default(),
                extract_tag(object, "VersionId"),
            )
        })
        .collect()
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn versioning_enabled_page() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Status>Enabled</Status></VersioningConfiguration>"#
        .to_string()
}

fn objects_page(page: usize, truncated: bool) -> String {
    let next_token = if truncated {
        format!("<NextContinuationToken>token-{page}</NextContinuationToken>")
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Name>fake-sigint-bucket</Name><Prefix></Prefix><KeyCount>1</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>{truncated}</IsTruncated>{next_token}<Contents><Key>page-{page}.txt</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified><ETag>&quot;0123456789abcdef0123456789abcdef&quot;</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Contents></ListBucketResult>"#
    )
}

#[cfg(target_family = "unix")]
fn versions_page(page: usize, truncated: bool) -> String {
    let next_markers = if truncated {
        format!(
            "<NextKeyMarker>page-{page}.txt</NextKeyMarker><NextVersionIdMarker>version-{page}</NextVersionIdMarker>"
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListVersionsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Name>fake-sigint-bucket</Name><Prefix></Prefix><KeyMarker></KeyMarker><VersionIdMarker></VersionIdMarker><MaxKeys>1000</MaxKeys><IsTruncated>{truncated}</IsTruncated>{next_markers}<Version><Key>page-{page}.txt</Key><VersionId>version-{page}</VersionId><IsLatest>true</IsLatest><LastModified>2026-01-01T00:00:00.000Z</LastModified><ETag>&quot;0123456789abcdef0123456789abcdef&quot;</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Version></ListVersionsResult>"#
    )
}

fn delete_result_page(deleted: &[(String, Option<String>)]) -> String {
    let mut entries = String::new();
    for (key, version_id) in deleted {
        let version_id_tag = version_id
            .as_ref()
            .map(|v| format!("<VersionId>{v}</VersionId>"))
            .unwrap_or_default();
        entries.push_str(&format!(
            "<Deleted><Key>{key}</Key>{version_id_tag}</Deleted>"
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">{entries}</DeleteResult>"#
    )
}

/// Spawn the s3rm binary pointed at the fake endpoint with static
/// credentials, so no AWS configuration on the host is consulted.
fn spawn_s3rm(endpoint: &str, extra_args: &[&str]) -> Child {
    let mut args = vec![
        "--target-endpoint-url",
        endpoint,
        "--target-force-path-style",
        "--target-access-key",
        "fake-access-key",
        "--target-secret-access-key",
        "fake-secret-key",
        "--target-region",
        "us-east-1",
        "--aws-config-file",
        "./test_data/test_config/config",
        "--aws-shared-credentials-file",
        "./test_data/test_config/credentials",
    ];
    args.extend_from_slice(extra_args);
    args.push("s3://fake-sigint-bucket/");

    Command::new(env!("CARGO_BIN_EXE_s3rm"))
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn s3rm")
}

/// Wait until the fake endpoint has served at least `at_least` requests
/// counted by `counter`, so the child is provably inside the pipeline (its
/// Ctrl+C handler is installed before the pipeline starts, thus long since
/// registered). Fails fast with the child's stderr if it exits early.
fn wait_for_count(child: &mut Child, counter: &AtomicUsize, at_least: usize, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while counter.load(Ordering::SeqCst) < at_least {
        if let Some(status) = child.try_wait().expect("failed to poll s3rm") {
            let stderr = read_stderr(child);
            panic!(
                "s3rm exited ({status:?}) before the fake S3 served {at_least} {what}\nstderr: {stderr}"
            );
        }
        assert!(
            Instant::now() < deadline,
            "fake S3 served only {} {what} before timeout",
            counter.load(Ordering::SeqCst)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for the child to exit within `deadline` — SIGINT must terminate
/// the process promptly, not hang it. Kills the child on timeout.
fn wait_with_deadline(child: &mut Child, deadline: Duration) -> std::process::ExitStatus {
    let end = Instant::now() + deadline;
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll s3rm") {
            return status;
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            panic!("s3rm did not exit within {deadline:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Read the child's stderr after it has exited.
fn read_stderr(child: &mut Child) -> String {
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    stderr
}

#[cfg(target_family = "unix")]
fn send_sigint(child: &Child) {
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("failed to send SIGINT to s3rm");
}

/// Shared body of the SIGINT tests: start an endless deletion, wait until
/// the child is provably mid-pipeline (`min_pages` list pages and
/// `min_deletes` delete requests served), interrupt it, and require a
/// graceful exit with code 130 — `code()` is `None` for a raw signal kill,
/// so this also proves the signal was caught rather than terminating the
/// process directly — and a quiet stderr. Returns the fake endpoint for
/// extra assertions.
#[cfg(target_family = "unix")]
fn assert_sigint_exits_130(
    kind: ListingKind,
    extra_args: &[&str],
    min_pages: usize,
    min_deletes: usize,
    label: &str,
) -> FakeS3 {
    let fake = spawn_fake_s3(kind, None);
    let mut child = spawn_s3rm(&fake.endpoint, extra_args);

    wait_for_count(&mut child, &fake.pages_served, min_pages, "list page(s)");
    if min_deletes > 0 {
        wait_for_count(
            &mut child,
            &fake.deletes_served,
            min_deletes,
            "delete request(s)",
        );
    }
    send_sigint(&child);

    let status = wait_with_deadline(&mut child, Duration::from_secs(15));
    let stderr = read_stderr(&mut child);

    assert_eq!(
        status.code(),
        Some(130),
        "[{label}] expected graceful exit 130 after SIGINT, got {status:?}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("s3rm failed"),
        "[{label}] SIGINT should terminate quietly\nstderr: {stderr}"
    );

    fake
}

/// Ctrl+C during a `--dry-run` listing: only list requests are ever in
/// flight, and none may turn into deletions during shutdown.
#[test]
#[cfg(target_family = "unix")]
fn sigint_during_dry_run_listing_exits_130() {
    let fake = assert_sigint_exits_130(
        ListingKind::Objects,
        &["--dry-run", "--max-parallel-listings", "1"],
        3,
        0,
        "dry-run",
    );
    assert_eq!(
        fake.deletes_served.load(Ordering::SeqCst),
        0,
        "a dry run must never send delete requests"
    );
}

/// Ctrl+C with the default configuration (`--force` alone): parallel
/// listing dispatch and batch-deletion buffering, exactly as a plain
/// `s3rm -f s3://bucket/` runs.
#[test]
#[cfg(target_family = "unix")]
fn sigint_during_default_parallel_listing_exits_130() {
    assert_sigint_exits_130(ListingKind::Objects, &["--force"], 3, 0, "default-parallel");
}

/// Ctrl+C while batch `DeleteObjects` requests are actively being sent
/// (`--batch-size 2` flushes a batch every two listed objects).
#[test]
#[cfg(target_family = "unix")]
fn sigint_during_batch_deletion_exits_130() {
    assert_sigint_exits_130(
        ListingKind::Objects,
        &[
            "--force",
            "--batch-size",
            "2",
            "--max-parallel-listings",
            "1",
        ],
        3,
        1,
        "batch-deletion",
    );
}

/// Ctrl+C while single-object `DeleteObject` requests (`--batch-size 1`)
/// are actively being sent.
#[test]
#[cfg(target_family = "unix")]
fn sigint_during_single_object_deletion_exits_130() {
    assert_sigint_exits_130(
        ListingKind::Objects,
        &[
            "--force",
            "--batch-size",
            "1",
            "--max-parallel-listings",
            "1",
        ],
        3,
        2,
        "single-deletion",
    );
}

/// Ctrl+C during `--delete-all-versions` (versioning prerequisite check,
/// `ListObjectVersions` paging, and versioned batch deletion).
#[test]
#[cfg(target_family = "unix")]
fn sigint_during_versions_deletion_exits_130() {
    assert_sigint_exits_130(
        ListingKind::Versions,
        &[
            "--force",
            "--delete-all-versions",
            "--batch-size",
            "2",
            "--max-parallel-listings",
            "1",
        ],
        3,
        1,
        "versions",
    );
}

/// Control: the same harness without SIGINT completes the full
/// list-and-delete run and exits 0 — the SIGINT handling must not affect
/// uninterrupted runs.
#[test]
fn deletion_without_sigint_exits_zero() {
    let fake = spawn_fake_s3(ListingKind::Objects, Some(3));
    let mut child = spawn_s3rm(&fake.endpoint, &["--force", "--max-parallel-listings", "1"]);

    let status = wait_with_deadline(&mut child, Duration::from_secs(30));
    let stderr = read_stderr(&mut child);

    assert_eq!(
        status.code(),
        Some(0),
        "expected exit 0, got {status:?}\nstderr: {stderr}"
    );
    assert_eq!(fake.pages_served.load(Ordering::SeqCst), 3);
    assert!(
        fake.deletes_served.load(Ordering::SeqCst) >= 1,
        "the run must have sent at least one delete request"
    );
    let deleted = fake.deleted_keys.lock().unwrap();
    for key in ["page-0.txt", "page-1.txt", "page-2.txt"] {
        assert!(
            deleted.iter().any(|deleted_key| deleted_key == key),
            "delete requests missing {key}; deleted: {deleted:?}"
        );
    }
}
