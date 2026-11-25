//! Batch deletion using the S3 DeleteObjects API.
//!
//! Groups objects into batches of up to 1000 and calls the S3 batch
//! delete API for high-throughput deletion.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::types::ObjectIdentifier;
use tracing::{debug, warn};

use crate::config::Config;
use crate::types::{DeletionStatistics, S3Object};

use super::{DeleteResult, DeletedKey, Deleter, FailedKey};

/// Maximum objects per batch DeleteObjects API call (S3 limit).
pub const MAX_BATCH_SIZE: usize = 1000;

/// Error code recorded for a requested object that a DeleteObjects response
/// neither reported as deleted nor as failed.
pub const NOT_REPORTED_ERROR_CODE: &str = "NotReported";

/// Error code recorded for objects whose DeleteObjects errors had different
/// codes that cannot be told apart (the message lists the codes).
pub const AMBIGUOUS_ERROR_CODE: &str = "Ambiguous";

/// Determines whether an S3 batch deletion error code is retryable.
///
/// Retryable errors are transient server-side issues that may succeed
/// on a subsequent attempt:
/// - `InternalError` / `ServiceUnavailable` — transient server errors
/// - `SlowDown` — throttling
/// - `RequestTimeout` — transient network/timeout
///
/// Non-retryable errors (e.g. `AccessDenied`, `NoSuchKey`) are permanent
/// and should not be retried.
pub(crate) fn is_retryable_error_code(code: &str) -> bool {
    matches!(
        code,
        "InternalError" | "SlowDown" | "ServiceUnavailable" | "RequestTimeout" | "unknown"
    )
}

/// Deletes objects in batches using the S3 DeleteObjects API.
///
/// Groups objects into batches of up to `batch_size` (max 1000) and
/// calls DeleteObjects for each batch. Partial failures are reported
/// via logging and the returned result contains per-key details.
pub struct BatchDeleter {
    target: crate::storage::Storage,
}

impl BatchDeleter {
    pub fn new(target: crate::storage::Storage) -> Self {
        Self { target }
    }

    /// Retries a failed key using individual DeleteObject calls with force-retry logic.
    ///
    /// When a batch DeleteObjects call reports a retryable error for a key,
    /// this method falls back to individual DeleteObject attempts with the
    /// configured retry count and interval.
    ///
    /// Returns `Ok(DeletedKey)` on success, or `Err(FailedKey)` if all retries
    /// are exhausted.
    async fn retry_failed_key_with_single_delete(
        &self,
        key: &str,
        version_id: Option<&str>,
        if_match_etag: Option<String>,
        code: &str,
        message: &str,
        config: &Config,
    ) -> std::result::Result<DeletedKey, FailedKey> {
        let force_retry_count = config.force_retry_config.force_retry_count;
        let force_retry_interval = config.force_retry_config.force_retry_interval_milliseconds;

        for attempt in 0..=force_retry_count {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(force_retry_interval)).await;
            }

            match self
                .target
                .delete_object(
                    key,
                    version_id.map(|v| v.to_string()),
                    if_match_etag.clone(),
                )
                .await
            {
                Ok(_) => {
                    return Ok(DeletedKey {
                        key: key.to_string(),
                        version_id: version_id.map(|v| v.to_string()),
                    });
                }
                Err(e) => {
                    warn!(
                        key = key,
                        version_id = version_id,
                        attempt = attempt + 1,
                        max_attempts = force_retry_count + 1,
                        error = %e,
                        "S3 DeleteObject fallback attempt {}/{} failed for key '{}'.",
                        attempt + 1, force_retry_count + 1, key,
                    );
                }
            }
        }

        warn!(
            key = key,
            version_id = version_id,
            code = code,
            message = message,
            "S3 DeleteObject fallback exhausted all {} retries for key '{}': {} ({}).",
            force_retry_count + 1,
            key,
            code,
            message,
        );

        Err(FailedKey {
            key: key.to_string(),
            version_id: version_id.map(|v| v.to_string()),
            error_code: code.to_string(),
            error_message: message.to_string(),
        })
    }

    /// Retries an object via single DeleteObject (using its own key, version ID
    /// and, when `if_match` is set, its own ETag) and records the outcome.
    async fn retry_and_record(
        &self,
        obj: &S3Object,
        code: &str,
        message: &str,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        let if_match_etag = if config.if_match {
            obj.e_tag().map(|s| s.to_string())
        } else {
            None
        };

        match self
            .retry_failed_key_with_single_delete(
                obj.key(),
                obj.version_id(),
                if_match_etag,
                code,
                message,
                config,
            )
            .await
        {
            Ok(deleted_key) => result.deleted.push(deleted_key),
            Err(failed_key) => {
                self.target
                    .send_stats(DeletionStatistics::DeleteError {
                        key: obj.key().to_string(),
                    })
                    .await;
                result.failed.push(failed_key);
            }
        }
    }

    /// Records a non-retryable failure for `obj` and reports it to the stats channel.
    async fn record_failure(
        &self,
        obj: &S3Object,
        code: &str,
        message: &str,
        result: &mut DeleteResult,
    ) {
        warn!(
            key = obj.key(),
            version_id = obj.version_id(),
            code = code,
            message = message,
            "S3 DeleteObjects partial failure for key '{}': {} ({}).",
            obj.key(),
            code,
            message,
        );
        self.target
            .send_stats(DeletionStatistics::DeleteError {
                key: obj.key().to_string(),
            })
            .await;
        result.failed.push(FailedKey {
            key: obj.key().to_string(),
            version_id: obj.version_id().map(|v| v.to_string()),
            error_code: code.to_string(),
            error_message: message.to_string(),
        });
    }

    /// Applies a reconciled [`Outcome`] to a requested object.
    async fn apply_outcome(
        &self,
        obj: &S3Object,
        outcome: Outcome,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        match outcome {
            Outcome::Deleted => result.deleted.push(DeletedKey {
                key: obj.key().to_string(),
                version_id: obj.version_id().map(|v| v.to_string()),
            }),
            Outcome::Failed { code, message } => {
                self.record_failure(obj, &code, &message, result).await
            }
            Outcome::Retry { code, message } => {
                self.retry_and_record(obj, &code, &message, config, result)
                    .await
            }
            Outcome::NotReported => {
                self.record_failure(
                    obj,
                    NOT_REPORTED_ERROR_CODE,
                    "DeleteObjects response did not determine the outcome of this object",
                    result,
                )
                .await
            }
        }
    }
}

/// What a DeleteObjects response says about one requested object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Reported as deleted.
    Deleted,
    /// Failed with a non-retryable error.
    Failed { code: String, message: String },
    /// Failed with a retryable error: fall back to a single DeleteObject.
    Retry { code: String, message: String },
    /// The response does not determine this object's outcome (not mentioned,
    /// or mentioned only ambiguously). Recorded as failed, never retried,
    /// because it may already have been deleted.
    NotReported,
}

/// One `Deleted` or `Error` entry of a DeleteObjects response.
struct Entry<'a> {
    version_id: Option<&'a str>,
    /// `None` for a `Deleted` entry; `(code, message)` for an `Error` entry.
    error: Option<(String, String)>,
}

/// Returns true if a response entry's version ID is compatible with a
/// requested object's: they must be equal when both are present; an entry
/// without a version ID, or a request without one (S3 may echo a
/// delete-marker version ID), is compatible.
fn versions_compatible(entry: Option<&str>, requested: Option<&str>) -> bool {
    match (entry, requested) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// The outcome for objects that `errors` refer to, when it is known that
/// the errors and the objects correspond one to one but not in which order.
///
/// If all errors share a code, that code is used; otherwise the code is
/// [`AMBIGUOUS_ERROR_CODE`] and the message lists the codes. If every code is retryable the
/// objects are retried; otherwise they are recorded as failed.
fn outcome_for_errors(errors: &[(String, String)]) -> Outcome {
    let (code, message) = if errors.iter().all(|(c, _)| *c == errors[0].0) {
        errors[0].clone()
    } else {
        let codes: Vec<&str> = errors.iter().map(|(c, _)| c.as_str()).collect();
        (
            AMBIGUOUS_ERROR_CODE.to_string(),
            format!(
                "DeleteObjects errors that cannot be told apart: {}",
                codes.join(", ")
            ),
        )
    };
    if errors.iter().all(|(c, _)| is_retryable_error_code(c)) {
        Outcome::Retry { code, message }
    } else {
        Outcome::Failed { code, message }
    }
}

/// Reconciles a DeleteObjects response with the requested objects of the
/// batch, returning one [`Outcome`] per requested object (same order).
///
/// Every outcome is about a requested object, never about a key taken from
/// the response, so a malformed response can neither cause a delete outside
/// the batch nor report one. An outcome is only derived when the response
/// determines it; otherwise the object is [`Outcome::NotReported`].
///
/// 1. Requested objects and response entries are grouped by exact key. An
///    entry only ever applies to objects in its own group.
/// 2. Within a group, an entry whose version ID equals exactly one
///    requested version applies to that version. If several entries claim
///    the same version (e.g. both `Deleted` and `Error`), the response
///    contradicts itself and that version is `NotReported`.
/// 3. The remaining entries of a group apply to the remaining objects only
///    when their numbers are equal, every entry is version-compatible with
///    every object, and the entries are all `Deleted` (the objects are
///    deleted) or all errors (see [`outcome_for_errors`]). Otherwise the
///    remaining objects are `NotReported`.
/// 4. Error entries without a key (missing or empty) apply, as in step 3,
///    to the objects no entry mentions at all, when their numbers are equal
///    and every other entry of the response was used (an unused entry, e.g.
///    one with a garbled or unrequested key, might be about one of those
///    objects). Otherwise those objects are `NotReported`. Entries naming a
///    key that was not requested never apply to any object.
///
/// Entries that end up unused are logged and otherwise ignored: they say
/// nothing reliable about a requested object.
pub(crate) fn reconcile(
    chunk: &[S3Object],
    response: &aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput,
) -> Vec<Outcome> {
    let mut outcomes: Vec<Option<Outcome>> = vec![None; chunk.len()];

    let mut groups: HashMap<String, (Vec<usize>, Vec<Entry<'_>>)> = HashMap::new();
    for (i, obj) in chunk.iter().enumerate() {
        groups.entry(obj.key().to_string()).or_default().0.push(i);
    }

    let mut loose_errors: Vec<(String, String)> = Vec::new();
    let mut ignored = 0usize;

    let deleted_entries = response
        .deleted()
        .iter()
        .map(|d| (d.key(), d.version_id(), None));
    let error_entries = response.errors().iter().map(|e| {
        (
            e.key(),
            e.version_id(),
            Some((
                e.code().unwrap_or("unknown").to_string(),
                e.message().unwrap_or("no message").to_string(),
            )),
        )
    });
    for (key, version_id, error) in deleted_entries.chain(error_entries) {
        let key = key.filter(|k| !k.is_empty());
        let group = key.and_then(|k| groups.get_mut(k));
        match (group, key, error) {
            (Some((_, entries)), _, error) => entries.push(Entry { version_id, error }),
            (None, None, Some(error)) => loose_errors.push(error),
            (None, _, error) => {
                // A key that was not requested (or a Deleted entry without a
                // key): the entry says nothing reliable about any requested
                // object.
                ignored += 1;
                warn!(
                    key = key,
                    version_id = version_id,
                    error = ?error,
                    "S3 DeleteObjects response has an entry for a key that was not requested; \
                     ignoring it.",
                );
            }
        }
    }

    let mut unmentioned: Vec<usize> = Vec::new();
    for (objects, entries) in groups.into_values() {
        if entries.is_empty() {
            unmentioned.extend(objects);
            continue;
        }

        // Step 2: exact version matches. A version claimed by more than one
        // entry (e.g. both Deleted and Error) is contradictory: NotReported.
        let mut claims: HashMap<usize, Vec<Entry<'_>>> = HashMap::new();
        let mut open_entries: Vec<Entry<'_>> = Vec::new();
        for entry in entries {
            let matches: Vec<usize> = match entry.version_id {
                Some(vid) => objects
                    .iter()
                    .copied()
                    .filter(|&i| chunk[i].version_id() == Some(vid))
                    .collect(),
                None => Vec::new(),
            };
            if let [i] = matches[..] {
                claims.entry(i).or_default().push(entry);
            } else {
                open_entries.push(entry);
            }
        }
        for (i, claimed) in &claims {
            outcomes[*i] = Some(match &claimed[..] {
                [entry] => match &entry.error {
                    None => Outcome::Deleted,
                    Some(error) => outcome_for_errors(std::slice::from_ref(error)),
                },
                _ => {
                    ignored += claimed.len();
                    Outcome::NotReported
                }
            });
        }
        let open_objects: Vec<usize> = objects
            .into_iter()
            .filter(|i| !claims.contains_key(i))
            .collect();

        // Step 3: the rest of the group, only when determined.
        if open_entries.is_empty() {
            continue;
        }
        let compatible = open_entries.iter().all(|e| {
            open_objects
                .iter()
                .all(|&i| versions_compatible(e.version_id, chunk[i].version_id()))
        });
        let errors: Vec<(String, String)> = open_entries
            .iter()
            .filter_map(|e| e.error.clone())
            .collect();
        let determined = compatible
            && open_entries.len() == open_objects.len()
            && (errors.is_empty() || errors.len() == open_entries.len());
        if determined {
            let outcome = if errors.is_empty() {
                Outcome::Deleted
            } else {
                outcome_for_errors(&errors)
            };
            for i in open_objects {
                outcomes[i] = Some(outcome.clone());
            }
        } else {
            ignored += open_entries.len();
        }
    }

    // Step 4: errors without a key, only if every other entry was used.
    if !loose_errors.is_empty() {
        if ignored == 0 && loose_errors.len() == unmentioned.len() {
            warn!(
                errors = loose_errors.len(),
                "S3 DeleteObjects returned {} error(s) without a key; attributing them \
                 to the object(s) the response does not mention.",
                loose_errors.len(),
            );
            let outcome = outcome_for_errors(&loose_errors);
            for &i in &unmentioned {
                outcomes[i] = Some(outcome.clone());
            }
        } else {
            ignored += loose_errors.len();
        }
    }

    if ignored > 0 {
        warn!(
            ignored_entries = ignored,
            "S3 DeleteObjects response has {} entry(ies) that cannot be matched to a requested \
             object with certainty; ignoring them.",
            ignored,
        );
    }

    outcomes
        .into_iter()
        .map(|o| o.unwrap_or(Outcome::NotReported))
        .collect()
}

#[async_trait]
impl Deleter for BatchDeleter {
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<DeleteResult> {
        let mut result = DeleteResult::default();

        if objects.is_empty() {
            return Ok(result);
        }

        let batch_size = (config.batch_size as usize).min(MAX_BATCH_SIZE);

        for chunk in objects.chunks(batch_size) {
            let identifiers: Vec<ObjectIdentifier> = chunk
                .iter()
                .map(|obj| {
                    let mut builder = ObjectIdentifier::builder().key(obj.key());
                    if let Some(vid) = obj.version_id() {
                        builder = builder.version_id(vid);
                    }
                    // Include ETag for conditional deletion when if_match is enabled.
                    // S3 DeleteObjects API supports per-object ETag conditions.
                    if config.if_match {
                        if let Some(etag) = obj.e_tag() {
                            builder = builder.e_tag(etag);
                        }
                    }
                    builder.build().expect("ObjectIdentifier build failed")
                })
                .collect();

            let batch_count = identifiers.len();

            debug!(
                batch_size = batch_count,
                "sending DeleteObjects batch request."
            );

            let response = self.target.delete_objects(identifiers).await?;

            let deleted_before = result.deleted.len();
            let failed_before = result.failed.len();

            let outcomes = reconcile(chunk, &response);
            for (obj, outcome) in chunk.iter().zip(outcomes) {
                // Retryable errors fall back to individual DeleteObject with
                // retries; non-retryable errors are warnings, which the caller
                // (ObjectDeleter) may promote with --warn-as-error.
                self.apply_outcome(obj, outcome, config, &mut result).await;
            }

            debug!(
                deleted = result.deleted.len() - deleted_before,
                failed = result.failed.len() - failed_before,
                "DeleteObjects batch completed."
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
    use aws_sdk_s3::types::{DeletedObject, Error as S3Error, ObjectVersion};

    #[test]
    fn max_batch_size_is_1000() {
        assert_eq!(MAX_BATCH_SIZE, 1000);
    }

    fn version(key: &str, vid: &str) -> S3Object {
        S3Object::Versioning(ObjectVersion::builder().key(key).version_id(vid).build())
    }

    fn deleted(key: &str, vid: Option<&str>) -> DeletedObject {
        let mut b = DeletedObject::builder().key(key);
        if let Some(v) = vid {
            b = b.version_id(v);
        }
        b.build()
    }

    fn error(key: Option<&str>, vid: Option<&str>, code: &str) -> S3Error {
        let mut b = S3Error::builder().code(code).message("m");
        if let Some(k) = key {
            b = b.key(k);
        }
        if let Some(v) = vid {
            b = b.version_id(v);
        }
        b.build()
    }

    fn failed(code: &str) -> Outcome {
        Outcome::Failed {
            code: code.to_string(),
            message: "m".to_string(),
        }
    }

    fn retry(code: &str) -> Outcome {
        Outcome::Retry {
            code: code.to_string(),
            message: "m".to_string(),
        }
    }

    #[test]
    fn reconcile_matches_exact_versions() {
        let chunk = [version("a", "v1"), version("a", "v2"), version("b", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", Some("v2")))
            .deleted(deleted("b", Some("v1")))
            .errors(error(Some("a"), Some("v1"), "AccessDenied"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![failed("AccessDenied"), Outcome::Deleted, Outcome::Deleted]
        );
    }

    #[test]
    fn reconcile_keeps_a_keyed_error_within_its_own_key() {
        // The ambiguous error names `a`; it must never be pinned on `b`.
        let chunk = [version("a", "v1"), version("a", "v2"), version("b", "v1")];
        let response = DeleteObjectsOutput::builder()
            .errors(error(Some("a"), None, "AccessDenied"))
            .errors(error(Some("a"), Some("v1"), "SlowDown"))
            .errors(error(Some("a"), Some("v2"), "SlowDown"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![retry("SlowDown"), retry("SlowDown"), Outcome::NotReported]
        );
    }

    #[test]
    fn reconcile_deleted_entries_without_version_cover_their_key_despite_keyless_error() {
        // Two Deleted entries for `a` without version IDs fully cover both
        // versions of `a`; the keyless error then applies to `b`, the only
        // object nothing mentions.
        let chunk = [version("a", "v1"), version("a", "v2"), version("b", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", None))
            .deleted(deleted("a", None))
            .errors(error(None, None, "InternalError"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![Outcome::Deleted, Outcome::Deleted, retry("InternalError")]
        );
    }

    #[test]
    fn reconcile_groups_by_exact_key() {
        // Keys that differ only in line endings are distinct objects.
        let chunk = [version("x\r\n", "v1"), version("x\n", "v2")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("x\r\n", None))
            .errors(error(Some("x\n"), None, "SlowDown"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![Outcome::Deleted, retry("SlowDown")]
        );
    }

    #[test]
    fn reconcile_keyless_error_not_attributed_when_another_entry_is_unused() {
        // The Deleted entry with an unrequested (e.g. garbled) key might be
        // about `b`, so the keyless error must not be pinned on `b`.
        let chunk = [version("a", "v1"), version("b", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", Some("v1")))
            .deleted(deleted("b-garbled", Some("v1")))
            .errors(error(None, None, "SlowDown"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![Outcome::Deleted, Outcome::NotReported]
        );
    }

    #[test]
    fn reconcile_mixed_codes_are_ambiguous() {
        let chunk = [version("a", "v1"), version("a", "v2")];
        let response = DeleteObjectsOutput::builder()
            .errors(error(Some("a"), None, "SlowDown"))
            .errors(error(Some("a"), None, "AccessDenied"))
            .build();
        let outcomes = reconcile(&chunk, &response);
        assert!(outcomes.iter().all(|o| matches!(
            o,
            Outcome::Failed { code, .. } if code == AMBIGUOUS_ERROR_CODE
        )));
        assert!(!is_retryable_error_code(AMBIGUOUS_ERROR_CODE));
    }

    #[test]
    fn reconcile_rejects_incompatible_version_ids() {
        let chunk = [version("a", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", Some("other")))
            .build();
        assert_eq!(reconcile(&chunk, &response), vec![Outcome::NotReported]);
    }

    #[test]
    fn reconcile_contradictory_entries_for_one_version_are_not_reported() {
        let chunk = [version("a", "v1"), version("b", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", Some("v1")))
            .deleted(deleted("b", Some("v1")))
            .errors(error(Some("a"), Some("v1"), "AccessDenied"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![Outcome::NotReported, Outcome::Deleted]
        );
    }

    #[test]
    fn reconcile_error_for_unrequested_key_is_never_attributed() {
        // `b` is unmentioned and there is exactly one error, but that error
        // names another key, so it must not be pinned on `b`.
        let chunk = [version("a", "v1"), version("b", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", Some("v1")))
            .errors(error(Some("zz"), None, "SlowDown"))
            .build();
        assert_eq!(
            reconcile(&chunk, &response),
            vec![Outcome::Deleted, Outcome::NotReported]
        );
    }

    #[test]
    fn reconcile_ignores_entries_for_keys_not_requested() {
        let chunk = [version("a", "v1")];
        let response = DeleteObjectsOutput::builder()
            .deleted(deleted("a", Some("v1")))
            .deleted(deleted("z", None))
            .errors(error(Some("zz"), None, "InternalError"))
            .build();
        // The error for `zz` is keyless for our purposes, but nothing is
        // unmentioned, so it is ignored.
        assert_eq!(reconcile(&chunk, &response), vec![Outcome::Deleted]);
    }
}
