//! High-load stress tests for the s3rm-rs pipeline.
//!
//! These tests verify correctness under high concurrency, large object counts,
//! and backpressure conditions that the standard unit/property tests do not cover.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use async_channel::Sender;
use async_trait::async_trait;
use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
use aws_sdk_s3::operation::head_object::HeadObjectOutput;
use aws_sdk_s3::types::{DeletedObject, ObjectIdentifier};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::deleter::ObjectDeleter;
use crate::stage::Stage;
use crate::storage::StorageTrait;
use crate::test_utils::{init_dummy_tracing_subscriber, make_test_config};
use crate::types::token::PipelineCancellationToken;
use crate::types::{DeletionStatistics, DeletionStatsReport, S3Object};

// ---------------------------------------------------------------------------
// Lightweight mock storage for stress tests
// ---------------------------------------------------------------------------

/// A minimal mock storage that always succeeds on all operations.
/// Tracks the number of delete_objects batch calls and delete_object single calls.
#[derive(Clone)]
struct StressMockStorage {
    stats_sender: Sender<DeletionStatistics>,
    batch_delete_call_count: Arc<AtomicU64>,
    single_delete_call_count: Arc<AtomicU64>,
}

impl StressMockStorage {
    fn new(stats_sender: Sender<DeletionStatistics>) -> Self {
        Self {
            stats_sender,
            batch_delete_call_count: Arc::new(AtomicU64::new(0)),
            single_delete_call_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[async_trait]
impl StorageTrait for StressMockStorage {
    fn is_express_onezone_storage(&self) -> bool {
        false
    }

    async fn list_objects(&self, _sender: &Sender<S3Object>, _max_keys: i32) -> Result<()> {
        Ok(())
    }

    async fn list_object_versions(&self, _sender: &Sender<S3Object>, _max_keys: i32) -> Result<()> {
        Ok(())
    }

    async fn head_object(
        &self,
        _key: &str,
        _version_id: Option<String>,
    ) -> Result<HeadObjectOutput> {
        Ok(HeadObjectOutput::builder().build())
    }

    async fn get_object_tagging(
        &self,
        _key: &str,
        _version_id: Option<String>,
    ) -> Result<GetObjectTaggingOutput> {
        Ok(GetObjectTaggingOutput::builder()
            .set_tag_set(Some(vec![]))
            .build()
            .unwrap())
    }

    async fn delete_object(
        &self,
        _key: &str,
        _version_id: Option<String>,
        _if_match: Option<String>,
    ) -> Result<DeleteObjectOutput> {
        self.single_delete_call_count
            .fetch_add(1, Ordering::Relaxed);
        Ok(DeleteObjectOutput::builder().build())
    }

    async fn delete_objects(&self, objects: Vec<ObjectIdentifier>) -> Result<DeleteObjectsOutput> {
        self.batch_delete_call_count.fetch_add(1, Ordering::Relaxed);

        let mut builder = DeleteObjectsOutput::builder();
        for ident in &objects {
            builder = builder.deleted(
                DeletedObject::builder()
                    .key(ident.key())
                    .set_version_id(ident.version_id().map(|v| v.to_string()))
                    .build(),
            );
        }
        Ok(builder.build())
    }

    async fn is_versioning_enabled(&self) -> Result<bool> {
        Ok(false)
    }

    fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
        None
    }

    fn get_stats_sender(&self) -> Sender<DeletionStatistics> {
        self.stats_sender.clone()
    }

    async fn send_stats(&self, stats: DeletionStatistics) {
        let _ = self.stats_sender.send(stats).await;
    }

    fn set_warning(&self) {}
}

// ---------------------------------------------------------------------------
// Helper: build a Stage with the given channels and mock storage
// ---------------------------------------------------------------------------

fn make_stress_stage(
    config: Config,
    mock: StressMockStorage,
    receiver: Option<async_channel::Receiver<S3Object>>,
    sender: Option<Sender<S3Object>>,
    cancellation_token: PipelineCancellationToken,
) -> Stage {
    Stage::new(
        config,
        Box::new(mock),
        receiver,
        sender,
        cancellation_token,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}

// ---------------------------------------------------------------------------
// Test 1: High-volume filter chain throughput
// ---------------------------------------------------------------------------

/// Push 50,000 objects through a bounded channel pipeline (simulating filter chain)
/// and verify no objects are lost.
#[tokio::test]
async fn stress_high_volume_channel_throughput() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const OBJECT_COUNT: usize = 50_000;

        // Simulate a filter chain with bounded channels
        let (tx1, rx1) = async_channel::bounded::<S3Object>(1000);
        let (tx2, rx2) = async_channel::bounded::<S3Object>(1000);

        // Stage 1: forward all objects (simulates a pass-through filter)
        let forwarder = tokio::spawn(async move {
            while let Ok(obj) = rx1.recv().await {
                tx2.send(obj).await.unwrap();
            }
            drop(tx2);
        });

        // Producer: send 50,000 objects
        let producer = tokio::spawn(async move {
            for i in 0..OBJECT_COUNT {
                let obj = S3Object::new(&format!("key/{i}"), 100);
                tx1.send(obj).await.unwrap();
            }
            drop(tx1);
        });

        // Consumer: count received objects
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            while let Ok(_obj) = rx2.recv().await {
                count += 1;
            }
            count
        });

        producer.await.unwrap();
        forwarder.await.unwrap();
        let received = consumer.await.unwrap();

        assert_eq!(
            received, OBJECT_COUNT,
            "Expected {OBJECT_COUNT} objects through channel pipeline, got {received}"
        );
    })
    .await;
    result.expect("stress_high_volume_channel_throughput timed out after 60s");
}

// ---------------------------------------------------------------------------
// Test 2: Concurrent worker stats accuracy
// ---------------------------------------------------------------------------

/// Spawn 64 tokio tasks that each call increment_deleted() and increment_failed()
/// 1,000 times. Verify final counts are exactly correct.
#[tokio::test]
async fn stress_concurrent_stats_accuracy() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const TASK_COUNT: u64 = 64;
        const OPS_PER_TASK: u64 = 1_000;

        let report = Arc::new(DeletionStatsReport::new());

        let mut handles = Vec::with_capacity(TASK_COUNT as usize);
        for _ in 0..TASK_COUNT {
            let report = Arc::clone(&report);
            handles.push(tokio::spawn(async move {
                for _ in 0..OPS_PER_TASK {
                    report.increment_deleted(42);
                    report.increment_failed();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let expected_deleted = TASK_COUNT * OPS_PER_TASK;
        let expected_bytes = TASK_COUNT * OPS_PER_TASK * 42;
        let expected_failed = TASK_COUNT * OPS_PER_TASK;

        let snapshot = report.snapshot();

        assert_eq!(
            snapshot.stats_deleted_objects, expected_deleted,
            "deleted objects: expected {expected_deleted}, got {}",
            snapshot.stats_deleted_objects
        );
        assert_eq!(
            snapshot.stats_deleted_bytes, expected_bytes,
            "deleted bytes: expected {expected_bytes}, got {}",
            snapshot.stats_deleted_bytes
        );
        assert_eq!(
            snapshot.stats_failed_objects, expected_failed,
            "failed objects: expected {expected_failed}, got {}",
            snapshot.stats_failed_objects
        );
    })
    .await;
    result.expect("stress_concurrent_stats_accuracy timed out after 60s");
}

// ---------------------------------------------------------------------------
// Test 3: Channel backpressure handling
// ---------------------------------------------------------------------------

/// Create a small bounded channel (capacity 10), spawn a fast producer sending
/// 10,000 objects and a slow consumer. Verify all objects are received.
#[tokio::test]
async fn stress_channel_backpressure() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const OBJECT_COUNT: usize = 10_000;
        const CHANNEL_CAPACITY: usize = 10;

        let (tx, rx) = async_channel::bounded::<S3Object>(CHANNEL_CAPACITY);

        // Fast producer
        let producer = tokio::spawn(async move {
            for i in 0..OBJECT_COUNT {
                let obj = S3Object::new(&format!("backpressure/{i}"), 64);
                tx.send(obj).await.unwrap();
            }
            drop(tx);
        });

        // Slow consumer: yield between receives to simulate slow processing
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            while let Ok(_obj) = rx.recv().await {
                count += 1;
                // Yield every 100 objects to simulate slow processing
                if count % 100 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            count
        });

        producer.await.unwrap();
        let received = consumer.await.unwrap();

        assert_eq!(
            received, OBJECT_COUNT,
            "Expected {OBJECT_COUNT} objects under backpressure, got {received}"
        );
    })
    .await;
    result.expect("stress_channel_backpressure timed out after 60s");
}

// ---------------------------------------------------------------------------
// Test 4: High-volume ObjectDeleter batch processing
// ---------------------------------------------------------------------------

/// Send 10,000 objects through a single ObjectDeleter worker with batch_size=1000.
/// Verify deletion stats show exactly 10,000 deleted objects.
#[tokio::test]
async fn stress_object_deleter_batch_processing() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const OBJECT_COUNT: usize = 10_000;

        let mut config = make_test_config();
        config.batch_size = 1000;

        let cancellation_token = CancellationToken::new();
        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let mock = StressMockStorage::new(stats_sender);

        // Input channel: objects flowing into the deleter
        let (obj_tx, obj_rx) = async_channel::bounded(1000);
        // Output channel: objects flowing out of the deleter to the next stage
        let (out_tx, out_rx) = async_channel::bounded(1000);

        let deletion_stats_report = Arc::new(DeletionStatsReport::new());
        let delete_counter = Arc::new(AtomicU64::new(0));

        let batch_call_count = Arc::clone(&mock.batch_delete_call_count);

        let stage = make_stress_stage(config, mock, Some(obj_rx), Some(out_tx), cancellation_token);

        let mut deleter =
            ObjectDeleter::new(stage, 0, Arc::clone(&deletion_stats_report), delete_counter);

        // Producer: send objects
        let producer = tokio::spawn(async move {
            for i in 0..OBJECT_COUNT {
                let obj = S3Object::new(&format!("batch/{i}"), 256);
                obj_tx.send(obj).await.unwrap();
            }
            drop(obj_tx);
        });

        // Consumer: drain the output channel
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            while let Ok(_) = out_rx.recv().await {
                count += 1;
            }
            count
        });

        // Run the deleter
        let delete_result = tokio::spawn(async move { deleter.delete().await });

        producer.await.unwrap();
        delete_result.await.unwrap().unwrap();
        // Close stats_receiver sender side implicitly when deleter and stage are dropped
        drop(stats_receiver);
        let forwarded = consumer.await.unwrap();

        let snapshot = deletion_stats_report.snapshot();

        assert_eq!(
            snapshot.stats_deleted_objects, OBJECT_COUNT as u64,
            "Expected {OBJECT_COUNT} deleted, got {}",
            snapshot.stats_deleted_objects
        );
        assert_eq!(
            snapshot.stats_failed_objects, 0,
            "Expected 0 failures, got {}",
            snapshot.stats_failed_objects
        );
        assert_eq!(
            forwarded, OBJECT_COUNT,
            "Expected {OBJECT_COUNT} forwarded to next stage, got {forwarded}"
        );

        // With batch_size=1000 and 10,000 objects, expect exactly 10 batch calls
        let batches = batch_call_count.load(Ordering::Relaxed);
        assert_eq!(batches, 10, "Expected 10 batch calls, got {batches}");
    })
    .await;
    result.expect("stress_object_deleter_batch_processing timed out after 60s");
}

// ---------------------------------------------------------------------------
// Test 5: Multiple workers sharing MPMC channel
// ---------------------------------------------------------------------------

/// Create an MPMC channel and spawn 16 ObjectDeleter workers consuming from it.
/// Send 10,000 objects. Verify total deleted == 10,000 via shared stats report.
#[tokio::test]
async fn stress_multiple_workers_mpmc() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const OBJECT_COUNT: usize = 10_000;
        const WORKER_COUNT: u16 = 16;

        let mut config = make_test_config();
        config.batch_size = 100;
        config.worker_size = WORKER_COUNT;

        let cancellation_token = CancellationToken::new();
        let deletion_stats_report = Arc::new(DeletionStatsReport::new());
        let delete_counter = Arc::new(AtomicU64::new(0));

        // MPMC channel: all workers receive from the same receiver
        let (obj_tx, obj_rx) = async_channel::bounded(500);
        // Each worker gets its own output sender into a shared output channel
        let (out_tx, out_rx) = async_channel::bounded(500);

        let mut worker_handles = Vec::with_capacity(WORKER_COUNT as usize);

        for i in 0..WORKER_COUNT {
            let (stats_sender, _stats_receiver) = async_channel::unbounded();
            let mock = StressMockStorage::new(stats_sender);

            let stage = make_stress_stage(
                config.clone(),
                mock,
                Some(obj_rx.clone()),
                Some(out_tx.clone()),
                cancellation_token.clone(),
            );

            let stats = Arc::clone(&deletion_stats_report);
            let counter = Arc::clone(&delete_counter);
            let mut deleter = ObjectDeleter::new(stage, i, stats, counter);

            worker_handles.push(tokio::spawn(async move { deleter.delete().await }));
        }

        // Drop the clones so channels close when workers finish
        drop(obj_rx);
        drop(out_tx);

        // Producer
        let producer = tokio::spawn(async move {
            for i in 0..OBJECT_COUNT {
                let obj = S3Object::new(&format!("mpmc/{i}"), 128);
                obj_tx.send(obj).await.unwrap();
            }
            drop(obj_tx);
        });

        // Consumer: drain output
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            while let Ok(_) = out_rx.recv().await {
                count += 1;
            }
            count
        });

        producer.await.unwrap();

        for h in worker_handles {
            h.await.unwrap().unwrap();
        }

        let forwarded = consumer.await.unwrap();
        let snapshot = deletion_stats_report.snapshot();

        assert_eq!(
            snapshot.stats_deleted_objects, OBJECT_COUNT as u64,
            "Expected {OBJECT_COUNT} total deleted across {WORKER_COUNT} workers, got {}",
            snapshot.stats_deleted_objects
        );
        assert_eq!(
            snapshot.stats_failed_objects, 0,
            "Expected 0 failures, got {}",
            snapshot.stats_failed_objects
        );
        assert_eq!(
            forwarded, OBJECT_COUNT,
            "Expected {OBJECT_COUNT} forwarded, got {forwarded}"
        );
    })
    .await;
    result.expect("stress_multiple_workers_mpmc timed out after 60s");
}

// ---------------------------------------------------------------------------
// Test 6: Filter chain under cancellation
// ---------------------------------------------------------------------------

/// Push objects through a channel, cancel the pipeline mid-stream, and verify
/// the filter/consumer exits cleanly without panic.
#[tokio::test]
async fn stress_filter_chain_cancellation() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const TOTAL_OBJECTS: usize = 50_000;
        const CANCEL_AFTER: usize = 5_000;

        let cancellation_token = CancellationToken::new();
        let (tx, rx) = async_channel::bounded::<S3Object>(100);

        let cancel_token = cancellation_token.clone();

        // Consumer that monitors cancellation
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            loop {
                tokio::select! {
                    recv_result = rx.recv() => {
                        match recv_result {
                            Ok(_) => {
                                count += 1;
                            }
                            Err(_) => break,
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                }
            }
            count
        });

        // Producer
        let cancel_trigger = cancellation_token.clone();
        let producer = tokio::spawn(async move {
            for i in 0..TOTAL_OBJECTS {
                if i == CANCEL_AFTER {
                    cancel_trigger.cancel();
                }
                // After cancellation, sends may fail because consumer stopped.
                let obj = S3Object::new(&format!("cancel/{i}"), 50);
                if tx.send(obj).await.is_err() {
                    break;
                }
            }
        });

        producer.await.unwrap();
        let received = consumer.await.unwrap();

        // We cancelled after producing CANCEL_AFTER objects.
        // Consumer should have received at least some objects but not all.
        assert!(
            received >= 1,
            "Consumer should have received at least 1 object, got {received}"
        );
        assert!(
            received <= TOTAL_OBJECTS,
            "Consumer received more than total: {received}"
        );
        assert!(
            cancellation_token.is_cancelled(),
            "Cancellation token should be cancelled"
        );
    })
    .await;
    result.expect("stress_filter_chain_cancellation timed out after 60s");
}

// ---------------------------------------------------------------------------
// Test 7: High-volume dry-run mode
// ---------------------------------------------------------------------------

/// Send 50,000 objects through ObjectDeleter in dry-run mode.
/// Verify stats show 50,000 deleted and mock storage received zero actual calls.
#[tokio::test]
async fn stress_high_volume_dry_run() {
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        init_dummy_tracing_subscriber();

        const OBJECT_COUNT: usize = 50_000;

        let mut config = make_test_config();
        config.batch_size = 1000;
        config.dry_run = true;

        let cancellation_token = CancellationToken::new();
        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let mock = StressMockStorage::new(stats_sender);

        let batch_calls = Arc::clone(&mock.batch_delete_call_count);
        let single_calls = Arc::clone(&mock.single_delete_call_count);

        let (obj_tx, obj_rx) = async_channel::bounded(1000);
        let (out_tx, out_rx) = async_channel::bounded(1000);

        let deletion_stats_report = Arc::new(DeletionStatsReport::new());
        let delete_counter = Arc::new(AtomicU64::new(0));

        let stage = make_stress_stage(config, mock, Some(obj_rx), Some(out_tx), cancellation_token);

        let mut deleter =
            ObjectDeleter::new(stage, 0, Arc::clone(&deletion_stats_report), delete_counter);

        // Producer
        let producer = tokio::spawn(async move {
            for i in 0..OBJECT_COUNT {
                let obj = S3Object::new(&format!("dryrun/{i}"), 512);
                obj_tx.send(obj).await.unwrap();
            }
            drop(obj_tx);
        });

        // Consumer: drain output
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            while let Ok(_) = out_rx.recv().await {
                count += 1;
            }
            count
        });

        // Run deleter
        let delete_handle = tokio::spawn(async move { deleter.delete().await });

        producer.await.unwrap();
        delete_handle.await.unwrap().unwrap();
        drop(stats_receiver);
        let forwarded = consumer.await.unwrap();

        let snapshot = deletion_stats_report.snapshot();

        assert_eq!(
            snapshot.stats_deleted_objects, OBJECT_COUNT as u64,
            "Dry-run should report {OBJECT_COUNT} deleted, got {}",
            snapshot.stats_deleted_objects
        );
        assert_eq!(
            snapshot.stats_failed_objects, 0,
            "Dry-run should have 0 failures, got {}",
            snapshot.stats_failed_objects
        );
        assert_eq!(
            forwarded, OBJECT_COUNT,
            "Dry-run should forward {OBJECT_COUNT} objects, got {forwarded}"
        );

        // The mock storage should have received ZERO actual delete calls
        assert_eq!(
            batch_calls.load(Ordering::Relaxed),
            0,
            "Dry-run should make 0 batch delete calls"
        );
        assert_eq!(
            single_calls.load(Ordering::Relaxed),
            0,
            "Dry-run should make 0 single delete calls"
        );
    })
    .await;
    result.expect("stress_high_volume_dry_run timed out after 60s");
}
