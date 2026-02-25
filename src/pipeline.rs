//! Deletion pipeline orchestrator.
//!
//! Adapted from s3sync's `pipeline/mod.rs`. This is the core orchestrator that
//! creates and connects all pipeline stages: List → Filter → Delete → Terminate.
//!
//! The pipeline uses streaming architecture with bounded async channels between
//! stages to minimize memory usage. Multiple ObjectDeleter workers run concurrently
//! using an MPMC (multi-producer, multi-consumer) channel pattern.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_channel::Receiver;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::deleter::ObjectDeleter;
use crate::filters::user_defined::UserDefinedFilter;
use crate::filters::{
    ExcludeRegexFilter, IncludeRegexFilter, LargerSizeFilter, MtimeAfterFilter, MtimeBeforeFilter,
    ObjectFilter, SmallerSizeFilter,
};
use crate::lister::ObjectLister;
use crate::safety::SafetyChecker;
use crate::stage::Stage;
use crate::storage::{self, Storage};
use crate::terminator::Terminator;
use crate::types::event_callback::{EventData, EventType};
use crate::types::token::PipelineCancellationToken;
use crate::types::{DeletionStatistics, DeletionStats, DeletionStatsReport, S3Object};

/// The core deletion pipeline orchestrator.
///
/// Adapted from s3sync's Pipeline struct, simplified for deletion-only operations:
/// - Single target storage (no source needed)
/// - Safety checks (confirmation prompts, dry-run) before execution
/// - ObjectDeleter workers instead of ObjectSyncer workers
///
/// ## Pipeline stages
///
/// ```text
/// ObjectLister → [Filters] → ObjectDeleter Workers (MPMC) → Terminator
/// ```
///
/// ## Usage
///
/// ```no_run
/// # // Example (will not actually run without AWS credentials)
/// # async fn example() {
/// # use s3rm_rs::{Config, DeletionPipeline, create_pipeline_cancellation_token};
/// # let config: Config = todo!();
/// let cancellation_token = create_pipeline_cancellation_token();
/// let mut pipeline = DeletionPipeline::new(config, cancellation_token).await;
/// pipeline.close_stats_sender();
/// pipeline.run().await;
/// if pipeline.has_error() {
///     eprintln!("{:?}", pipeline.get_errors_and_consume().unwrap()[0]);
/// }
/// # }
/// ```
pub struct DeletionPipeline {
    config: Config,
    target: Storage,
    cancellation_token: PipelineCancellationToken,
    stats_receiver: Receiver<DeletionStatistics>,
    has_error: Arc<AtomicBool>,
    has_panic: Arc<AtomicBool>,
    has_warning: Arc<AtomicBool>,
    errors: Arc<Mutex<VecDeque<anyhow::Error>>>,
    ready: bool,
    prerequisites_checked: bool,
    deletion_stats_report: Arc<DeletionStatsReport>,
}

impl DeletionPipeline {
    /// Create a new DeletionPipeline.
    ///
    /// Initializes the S3 storage, stats channel, and error tracking.
    /// The pipeline is ready to run after creation.
    pub async fn new(config: Config, cancellation_token: PipelineCancellationToken) -> Self {
        let has_warning = Arc::new(AtomicBool::new(false));

        // Create unbounded stats channel (same as s3sync)
        let (stats_sender, stats_receiver) = async_channel::unbounded();

        // Create target storage
        let target = storage::create_storage(
            config.clone(),
            cancellation_token.clone(),
            stats_sender,
            has_warning.clone(),
        )
        .await;

        Self {
            config,
            target,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        }
    }

    /// Run the deletion pipeline.
    ///
    /// 1. Fire PIPELINE_START event
    /// 2. Check prerequisites (safety confirmation)
    /// 3. Execute pipeline stages (list → filter → delete → terminate)
    /// 4. Shutdown and fire PIPELINE_END / PIPELINE_ERROR events
    pub async fn run(&mut self) {
        assert!(self.ready, "DeletionPipeline::run() called more than once");
        self.ready = false;

        // Fire PIPELINE_START event
        self.config
            .event_manager
            .trigger_event(EventData::new(EventType::PIPELINE_START))
            .await;

        // Check prerequisites (confirmation prompt, safety checks, versioning)
        // Skip if already checked via explicit check_prerequisites() call.
        if !self.prerequisites_checked {
            if let Err(e) = self.check_prerequisites().await {
                self.record_error(e);
                self.shutdown();
                self.fire_completion_events().await;
                return;
            }
        }

        // Run the pipeline stages
        self.execute_pipeline().await;

        // Shutdown
        self.shutdown();

        // Fire completion events
        self.fire_completion_events().await;
    }

    /// Check if any error occurred during the pipeline execution.
    pub fn has_error(&self) -> bool {
        self.has_error.load(Ordering::SeqCst)
    }

    /// Check if any spawned task panicked during the pipeline execution.
    pub fn has_panic(&self) -> bool {
        self.has_panic.load(Ordering::SeqCst)
    }

    /// Check if any warning occurred during the pipeline execution.
    pub fn has_warning(&self) -> bool {
        self.has_warning.load(Ordering::SeqCst)
    }

    /// Consume and return all accumulated errors.
    ///
    /// Returns `None` if no errors occurred.
    pub fn get_errors_and_consume(&self) -> Option<Vec<anyhow::Error>> {
        if !self.has_error() {
            return None;
        }
        let mut error_list = self.errors.lock().unwrap();
        let mut errors = Vec::with_capacity(error_list.len());
        while let Some(e) = error_list.pop_front() {
            errors.push(e);
        }
        Some(errors)
    }

    /// Get error messages without consuming them.
    ///
    /// Returns `None` if no errors occurred.
    pub fn get_error_messages(&self) -> Option<Vec<String>> {
        if !self.has_error() {
            return None;
        }
        let error_list = self.errors.lock().unwrap();
        Some(error_list.iter().map(|e| e.to_string()).collect())
    }

    /// Get the stats receiver for progress reporting.
    ///
    /// The progress reporter reads from this channel to display
    /// real-time deletion progress.
    pub fn get_stats_receiver(&self) -> Receiver<DeletionStatistics> {
        self.stats_receiver.clone()
    }

    /// Get a snapshot of the current deletion statistics.
    pub fn get_deletion_stats(&self) -> DeletionStats {
        self.deletion_stats_report.snapshot()
    }

    /// Close the stats sender to signal the progress reporter to finish.
    ///
    /// Call this before `run()` if you don't need progress reporting,
    /// to free the stats channel resources.
    pub fn close_stats_sender(&self) {
        self.target.get_stats_sender().close();
    }

    // -----------------------------------------------------------------------
    // Internal methods
    // -----------------------------------------------------------------------

    /// Check safety prerequisites before running the pipeline.
    ///
    /// This includes the confirmation prompt, dry-run check, force flag,
    /// and versioning validation. Call this before `run()` if you need
    /// to perform actions (e.g., starting a progress indicator) between
    /// the confirmation prompt and pipeline execution.
    ///
    /// If not called explicitly, `run()` will call it automatically.
    pub async fn check_prerequisites(&mut self) -> Result<()> {
        // Safety checks (confirmation prompt, dry-run, force flag)
        let checker = SafetyChecker::new(&self.config);
        checker.check_before_deletion()?;

        // If delete_all_versions is set but the bucket is not versioned,
        // silently clear the flag and proceed with normal deletion (Requirement 5.6).
        if self.config.delete_all_versions {
            let versioning_enabled = self.target.is_versioning_enabled().await?;
            if !versioning_enabled {
                self.config.delete_all_versions = false;
            }
        }

        self.prerequisites_checked = true;
        Ok(())
    }

    /// Execute the pipeline stages: list → filter → delete → terminate.
    async fn execute_pipeline(&self) {
        // Stage 1: List objects
        let listed_objects = self.list_target();

        // Stage 2: Chain filter stages
        let filtered_objects = self.filter_objects(listed_objects);

        // Stage 3: Spawn deletion workers
        let deleted_objects = self.delete_objects(filtered_objects);

        // Stage 4: Terminate (drain final output)
        let terminator_handle = self.terminate(deleted_objects);

        // Wait for the terminator to finish (all upstream stages complete)
        if let Err(e) = terminator_handle.await {
            self.has_panic.store(true, Ordering::SeqCst);
            error!("terminator task panicked: {}", e);
            self.record_error(anyhow::anyhow!("terminator task panicked: {}", e));
        }

        // Promote warnings to errors if configured (checked once after all workers complete)
        if self.config.warn_as_error && self.has_warning.load(Ordering::SeqCst) {
            self.record_error(anyhow::anyhow!(
                "warnings promoted to errors (--warn-as-error)"
            ));
        }
    }

    /// Record an error and set the error flag.
    fn record_error(&self, error: anyhow::Error) {
        self.has_error.store(true, Ordering::SeqCst);
        self.errors.lock().unwrap().push_back(error);
    }

    /// Shutdown: close stats sender.
    fn shutdown(&self) {
        self.close_stats_sender();
    }

    /// Fire PIPELINE_END or PIPELINE_ERROR events.
    async fn fire_completion_events(&self) {
        if self.has_error() {
            let unknown = "Unknown error".to_string();
            let mut event_data = EventData::new(EventType::PIPELINE_ERROR);
            event_data.message = Some(
                self.get_error_messages()
                    .unwrap_or_default()
                    .first()
                    .unwrap_or(&unknown)
                    .to_string(),
            );
            self.config.event_manager.trigger_event(event_data).await;
        }

        if self.cancellation_token.is_cancelled() {
            let mut event_data = EventData::new(EventType::DELETE_CANCEL);
            event_data.message = Some("Pipeline was cancelled".to_string());
            self.config.event_manager.trigger_event(event_data).await;
        }

        self.config
            .event_manager
            .trigger_event(EventData::new(EventType::PIPELINE_END))
            .await;
    }

    // -----------------------------------------------------------------------
    // Stage creation helpers (adapted from s3sync)
    // -----------------------------------------------------------------------

    /// Create an SPSC (single-producer, single-consumer) stage.
    ///
    /// Returns the Stage and the next stage's receiver.
    /// Used for filter stages where one task reads and one task writes.
    fn create_spsc_stage(
        &self,
        previous_stage_receiver: Option<Receiver<S3Object>>,
        has_warning: Arc<AtomicBool>,
    ) -> (Stage, Receiver<S3Object>) {
        let (sender, next_stage_receiver) =
            async_channel::bounded::<S3Object>(self.config.object_listing_queue_size as usize);

        let stage = Stage::new(
            self.config.clone(),
            dyn_clone::clone_box(&*self.target),
            previous_stage_receiver,
            Some(sender),
            self.cancellation_token.clone(),
            has_warning,
        );

        (stage, next_stage_receiver)
    }

    /// Create an MPMC (multi-producer, multi-consumer) stage.
    ///
    /// Takes shared sender and receiver channels. Used for ObjectDeleter workers
    /// where multiple workers share the same input and output channels.
    fn create_mpmc_stage(
        &self,
        sender: async_channel::Sender<S3Object>,
        receiver: Receiver<S3Object>,
        has_warning: Arc<AtomicBool>,
    ) -> Stage {
        Stage::new(
            self.config.clone(),
            dyn_clone::clone_box(&*self.target),
            Some(receiver),
            Some(sender),
            self.cancellation_token.clone(),
            has_warning,
        )
    }

    /// Spawn a filter stage task with error handling.
    ///
    /// Uses the double-spawn pattern from s3sync to catch panics.
    fn spawn_filter(&self, filter: Box<dyn ObjectFilter + Send + Sync>) {
        let has_error = self.has_error.clone();
        let has_panic = self.has_panic.clone();
        let error_list = self.errors.clone();
        let cancellation_token = self.cancellation_token.clone();

        tokio::spawn(async move {
            let join_result = tokio::spawn(async move { filter.filter().await }).await;

            match join_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    cancellation_token.cancel();
                    has_error.store(true, Ordering::SeqCst);
                    error!("filter stage failed: {}", e);
                    error_list.lock().unwrap().push_back(e);
                }
                Err(e) => {
                    cancellation_token.cancel();
                    has_error.store(true, Ordering::SeqCst);
                    has_panic.store(true, Ordering::SeqCst);
                    error!("filter task panicked: {}", e);
                    error_list
                        .lock()
                        .unwrap()
                        .push_back(anyhow::anyhow!("filter task panicked: {}", e));
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // Pipeline stages
    // -----------------------------------------------------------------------

    /// Stage 1: Spawn the ObjectLister to list target objects.
    fn list_target(&self) -> Receiver<S3Object> {
        let (stage, receiver) = self.create_spsc_stage(None, self.has_warning.clone());

        let max_keys = self.config.max_keys;
        let has_error = self.has_error.clone();
        let has_panic = self.has_panic.clone();
        let error_list = self.errors.clone();
        let cancellation_token = self.cancellation_token.clone();

        tokio::spawn(async move {
            let lister = ObjectLister::new(stage);
            let join_result = tokio::spawn(async move { lister.list_target(max_keys).await }).await;

            match join_result {
                Ok(Ok(())) => {
                    debug!("object lister completed successfully.");
                }
                Ok(Err(e)) => {
                    cancellation_token.cancel();
                    has_error.store(true, Ordering::SeqCst);
                    error!("object lister failed: {}", e);
                    error_list.lock().unwrap().push_back(e);
                }
                Err(e) => {
                    cancellation_token.cancel();
                    has_error.store(true, Ordering::SeqCst);
                    has_panic.store(true, Ordering::SeqCst);
                    error!("object lister task panicked: {}", e);
                    error_list
                        .lock()
                        .unwrap()
                        .push_back(anyhow::anyhow!("object lister task panicked: {}", e));
                }
            }
        });

        receiver
    }

    /// Stage 2: Chain filter stages based on configuration.
    ///
    /// Each filter is conditionally inserted only if its corresponding
    /// configuration is set. Filters are chained in this order:
    /// 1. MtimeBeforeFilter
    /// 2. MtimeAfterFilter
    /// 3. SmallerSizeFilter
    /// 4. LargerSizeFilter
    /// 5. IncludeRegexFilter
    /// 6. ExcludeRegexFilter
    /// 7. UserDefinedFilter (Lua/Rust callback)
    fn filter_objects(&self, objects_list: Receiver<S3Object>) -> Receiver<S3Object> {
        let mut previous_stage_receiver = objects_list;

        // MtimeBeforeFilter
        if self.config.filter_config.before_time.is_some() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());
            self.spawn_filter(Box::new(MtimeBeforeFilter::new(stage)));
            previous_stage_receiver = new_receiver;
        }

        // MtimeAfterFilter
        if self.config.filter_config.after_time.is_some() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());
            self.spawn_filter(Box::new(MtimeAfterFilter::new(stage)));
            previous_stage_receiver = new_receiver;
        }

        // SmallerSizeFilter
        if self.config.filter_config.smaller_size.is_some() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());
            self.spawn_filter(Box::new(SmallerSizeFilter::new(stage)));
            previous_stage_receiver = new_receiver;
        }

        // LargerSizeFilter
        if self.config.filter_config.larger_size.is_some() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());
            self.spawn_filter(Box::new(LargerSizeFilter::new(stage)));
            previous_stage_receiver = new_receiver;
        }

        // IncludeRegexFilter
        if self.config.filter_config.include_regex.is_some() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());
            self.spawn_filter(Box::new(IncludeRegexFilter::new(stage)));
            previous_stage_receiver = new_receiver;
        }

        // ExcludeRegexFilter
        if self.config.filter_config.exclude_regex.is_some() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());
            self.spawn_filter(Box::new(ExcludeRegexFilter::new(stage)));
            previous_stage_receiver = new_receiver;
        }

        // UserDefinedFilter (Lua/Rust callback)
        // Only insert this stage when a filter callback is registered.
        // UserDefinedFilter has its own filter() method (not ObjectFilter trait),
        // so we spawn it directly instead of via spawn_filter().
        if self.config.filter_manager.is_callback_registered() {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());

            let has_error = self.has_error.clone();
            let has_panic = self.has_panic.clone();
            let error_list = self.errors.clone();
            let cancellation_token = self.cancellation_token.clone();

            tokio::spawn(async move {
                let filter = UserDefinedFilter::new(stage);
                let join_result = tokio::spawn(async move { filter.filter().await }).await;

                match join_result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        cancellation_token.cancel();
                        has_error.store(true, Ordering::SeqCst);
                        error!("user defined filter failed: {}", e);
                        error_list.lock().unwrap().push_back(e);
                    }
                    Err(e) => {
                        cancellation_token.cancel();
                        has_error.store(true, Ordering::SeqCst);
                        has_panic.store(true, Ordering::SeqCst);
                        error!("user defined filter task panicked: {}", e);
                        error_list
                            .lock()
                            .unwrap()
                            .push_back(anyhow::anyhow!("user defined filter panicked: {}", e));
                    }
                }
            });

            previous_stage_receiver = new_receiver;
        }

        previous_stage_receiver
    }

    /// Stage 3: Spawn ObjectDeleter workers using MPMC pattern.
    ///
    /// Creates a shared input channel and spawns `worker_size` concurrent
    /// ObjectDeleter workers, each reading from the same channel.
    fn delete_objects(&self, objects_to_be_deleted: Receiver<S3Object>) -> Receiver<S3Object> {
        let (sender, next_stage_receiver) =
            async_channel::bounded::<S3Object>(self.config.object_listing_queue_size as usize);
        let delete_counter = Arc::new(AtomicU64::new(0));

        for worker_index in 0..self.config.worker_size {
            let stage = self.create_mpmc_stage(
                sender.clone(),
                objects_to_be_deleted.clone(),
                self.has_warning.clone(),
            );

            let mut object_deleter = ObjectDeleter::new(
                stage,
                worker_index,
                self.deletion_stats_report.clone(),
                delete_counter.clone(),
            );

            let has_error = self.has_error.clone();
            let has_panic = self.has_panic.clone();
            let error_list = self.errors.clone();
            let cancellation_token = self.cancellation_token.clone();

            tokio::spawn(async move {
                let join_result = tokio::spawn(async move { object_deleter.delete().await }).await;

                match join_result {
                    Ok(Ok(())) => {
                        debug!(worker_index, "delete worker completed successfully.");
                    }
                    Ok(Err(e)) => {
                        // Check if this is a cancellation (e.g. max-delete threshold)
                        if crate::types::error::is_cancelled_error(&e) {
                            info!(worker_index, "delete worker cancelled.");
                        } else {
                            cancellation_token.cancel();
                            has_error.store(true, Ordering::SeqCst);
                            error!(worker_index, "delete worker failed: {}", e);
                            error_list.lock().unwrap().push_back(e);
                        }
                    }
                    Err(e) => {
                        cancellation_token.cancel();
                        has_error.store(true, Ordering::SeqCst);
                        has_panic.store(true, Ordering::SeqCst);
                        error!(worker_index, "delete worker task panicked: {}", e);
                        error_list
                            .lock()
                            .unwrap()
                            .push_back(anyhow::anyhow!("delete worker panicked: {}", e));
                    }
                }
            });
        }

        // Drop the cloned sender so the channel closes when all workers finish
        drop(sender);

        next_stage_receiver
    }

    /// Stage 4: Spawn the Terminator to drain the final output channel.
    fn terminate(&self, deleted_objects: Receiver<S3Object>) -> JoinHandle<()> {
        let terminator = Terminator::new(deleted_objects);
        tokio::spawn(async move {
            terminator.terminate().await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TracingConfig;
    use crate::filters::tests::{create_mock_storage, create_test_config};
    use crate::storage::StorageTrait;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use crate::types::DeletionStatistics;
    use crate::types::token::create_pipeline_cancellation_token;
    use async_trait::async_trait;
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::Object;
    use proptest::prelude::*;
    use std::sync::atomic::AtomicBool;

    // -----------------------------------------------------------------------
    // Stage creation and channel communication tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn spsc_stage_creates_connected_channels() {
        init_dummy_tracing_subscriber();

        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token: cancellation_token.clone(),
            stats_receiver: async_channel::unbounded().1,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
        let (stage, output_receiver) =
            pipeline.create_spsc_stage(Some(input_receiver), pipeline.has_warning.clone());

        // Verify the stage has the right channels
        assert!(stage.receiver.is_some());
        assert!(stage.sender.is_some());

        // Send an object through
        let obj = S3Object::NotVersioning(Object::builder().key("test-key").build());
        input_sender.send(obj).await.unwrap();
        input_sender.close();

        // Read from stage receiver, forward via stage sender
        let received = stage.receiver.as_ref().unwrap().recv().await.unwrap();
        assert_eq!(received.key(), "test-key");
        stage.sender.as_ref().unwrap().send(received).await.unwrap();

        // Should appear on the output
        let output = output_receiver.recv().await.unwrap();
        assert_eq!(output.key(), "test-key");
    }

    #[tokio::test]
    async fn mpmc_stage_shares_channels() {
        init_dummy_tracing_subscriber();

        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token: cancellation_token.clone(),
            stats_receiver: async_channel::unbounded().1,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        let (shared_sender, shared_receiver) = async_channel::bounded::<S3Object>(10);
        let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

        // Create two MPMC stages sharing the same channels
        let stage1 = pipeline.create_mpmc_stage(
            output_sender.clone(),
            shared_receiver.clone(),
            pipeline.has_warning.clone(),
        );
        let stage2 = pipeline.create_mpmc_stage(
            output_sender,
            shared_receiver,
            pipeline.has_warning.clone(),
        );

        // Both stages should have receivers and senders
        assert!(stage1.receiver.is_some());
        assert!(stage1.sender.is_some());
        assert!(stage2.receiver.is_some());
        assert!(stage2.sender.is_some());

        // Send two objects
        let obj1 = S3Object::NotVersioning(Object::builder().key("key1").build());
        let obj2 = S3Object::NotVersioning(Object::builder().key("key2").build());
        shared_sender.send(obj1).await.unwrap();
        shared_sender.send(obj2).await.unwrap();

        // Both objects should be receivable (by either stage)
        let r1 = stage1.receiver.as_ref().unwrap().recv().await.unwrap();
        let r2 = stage2.receiver.as_ref().unwrap().recv().await.unwrap();

        let mut keys: Vec<String> = vec![r1.key().to_string(), r2.key().to_string()];
        keys.sort();
        assert_eq!(keys, vec!["key1", "key2"]);
    }

    // -----------------------------------------------------------------------
    // Error and stats methods tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn has_error_initially_false() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        assert!(!pipeline.has_error());
        assert!(!pipeline.has_warning());
        assert!(pipeline.get_errors_and_consume().is_none());
    }

    #[tokio::test]
    async fn record_error_sets_flag_and_stores_error() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.record_error(anyhow::anyhow!("test error"));
        assert!(pipeline.has_error());

        let errors = pipeline.get_errors_and_consume().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "test error");

        // After consuming, errors list is empty but flag remains
        assert!(pipeline.has_error());
    }

    #[tokio::test]
    async fn deletion_stats_initially_zero() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 0);
        assert_eq!(stats.stats_failed_objects, 0);
        assert_eq!(stats.stats_deleted_bytes, 0);
    }

    // -----------------------------------------------------------------------
    // Cancellation test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn cancellation_propagates_to_stages() {
        init_dummy_tracing_subscriber();

        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token: cancellation_token.clone(),
            stats_receiver: async_channel::unbounded().1,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        // Create a stage and verify cancellation propagates
        let (stage, _receiver) = pipeline.create_spsc_stage(None, pipeline.has_warning.clone());

        assert!(!stage.cancellation_token.is_cancelled());
        cancellation_token.cancel();
        assert!(stage.cancellation_token.is_cancelled());
    }

    // -----------------------------------------------------------------------
    // Pipeline integration test with mock storage
    // -----------------------------------------------------------------------

    /// Mock storage that sends specific objects when list_objects is called.
    #[derive(Clone)]
    struct ListingMockStorage {
        objects: Vec<S3Object>,
        stats_sender: async_channel::Sender<DeletionStatistics>,
        has_warning: Arc<AtomicBool>,
    }

    #[async_trait]
    impl StorageTrait for ListingMockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }

        async fn list_objects(
            &self,
            sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            for obj in &self.objects {
                sender.send(obj.clone()).await.unwrap();
            }
            Ok(())
        }

        async fn list_object_versions(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            Ok(())
        }

        async fn head_object(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
            Ok(aws_sdk_s3::operation::head_object::HeadObjectOutput::builder().build())
        }

        async fn get_object_tagging(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput> {
            Ok(
                aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput::builder()
                    .build()
                    .unwrap(),
            )
        }

        async fn delete_object(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> Result<aws_sdk_s3::operation::delete_object::DeleteObjectOutput> {
            Ok(aws_sdk_s3::operation::delete_object::DeleteObjectOutput::builder().build())
        }

        async fn delete_objects(
            &self,
            objects: Vec<aws_sdk_s3::types::ObjectIdentifier>,
        ) -> Result<aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput> {
            use aws_sdk_s3::types::DeletedObject;
            let deleted: Vec<DeletedObject> = objects
                .into_iter()
                .map(|oi| {
                    DeletedObject::builder()
                        .key(oi.key())
                        .set_version_id(oi.version_id().map(|s| s.to_string()))
                        .build()
                })
                .collect();
            Ok(
                aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput::builder()
                    .set_deleted(Some(deleted))
                    .build(),
            )
        }

        async fn is_versioning_enabled(&self) -> Result<bool> {
            Ok(false)
        }

        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }

        fn get_stats_sender(&self) -> async_channel::Sender<DeletionStatistics> {
            self.stats_sender.clone()
        }

        async fn send_stats(&self, stats: DeletionStatistics) {
            let _ = self.stats_sender.send(stats).await;
        }

        fn set_warning(&self) {
            self.has_warning
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn pipeline_runs_with_mock_storage_dry_run() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("file1.txt")
                    .size(100)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("file2.txt")
                    .size(200)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.dry_run = true;
        config.force = true;
        config.batch_size = 1; // Use single deleter for simplicity

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Pipeline should complete without errors in dry-run mode
        assert!(!pipeline.has_error());
    }

    #[tokio::test]
    async fn pipeline_runs_with_mock_storage_and_deletes() {
        init_dummy_tracing_subscriber();

        let objects = vec![S3Object::NotVersioning(
            Object::builder()
                .key("delete-me.txt")
                .size(50)
                .last_modified(DateTime::from_secs(0))
                .build(),
        )];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true; // Skip confirmation
        config.batch_size = 1;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Should complete without errors
        assert!(!pipeline.has_error());

        // Stats should show the deletion
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 1);
        assert_eq!(stats.stats_deleted_bytes, 50);
    }

    #[tokio::test]
    async fn pipeline_empty_listing_completes() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects: vec![],
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        assert!(!pipeline.has_error());
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 0);
    }

    #[tokio::test]
    async fn pipeline_cancellation_stops_processing() {
        init_dummy_tracing_subscriber();

        // Create objects but cancel immediately
        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("key1")
                    .size(10)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("key2")
                    .size(20)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("key3")
                    .size(30)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;

        let cancellation_token = create_pipeline_cancellation_token();

        // Cancel before running
        cancellation_token.cancel();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // A pre-cancelled pipeline must not delete any objects
        let report = pipeline.deletion_stats_report.clone();
        assert_eq!(
            report.stats_deleted_objects.load(Ordering::Relaxed),
            0,
            "Pre-cancelled pipeline must not delete objects"
        );
        assert_eq!(
            report.stats_deleted_bytes.load(Ordering::Relaxed),
            0,
            "Pre-cancelled pipeline must not report deleted bytes"
        );
        assert!(
            !pipeline.has_error(),
            "Clean cancellation should not set error flag"
        );
    }

    #[tokio::test]
    async fn pipeline_with_filters() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("include-me.log")
                    .size(100)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("exclude-me.txt")
                    .size(200)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.filter_config.include_regex = Some(fancy_regex::Regex::new(r"\.log$").unwrap());

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        assert!(!pipeline.has_error());
        // Only the .log file should be deleted
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 1);
        assert_eq!(stats.stats_deleted_bytes, 100);
    }

    #[tokio::test]
    async fn pipeline_multiple_workers() {
        init_dummy_tracing_subscriber();

        let mut objects = Vec::new();
        for i in 0..20 {
            objects.push(S3Object::NotVersioning(
                Object::builder()
                    .key(format!("file{i}.txt"))
                    .size(10)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ));
        }

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 4;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        assert!(!pipeline.has_error());
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 20);
        assert_eq!(stats.stats_deleted_bytes, 200);
    }

    #[tokio::test]
    async fn pipeline_batch_deletion() {
        init_dummy_tracing_subscriber();

        let mut objects = Vec::new();
        for i in 0..5 {
            objects.push(S3Object::NotVersioning(
                Object::builder()
                    .key(format!("batch{i}.txt"))
                    .size(25)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ));
        }

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1000; // Use batch mode

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        assert!(!pipeline.has_error());
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 5);
        assert_eq!(stats.stats_deleted_bytes, 125);
    }

    #[tokio::test]
    #[should_panic(expected = "called more than once")]
    async fn pipeline_panics_on_double_run() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects: vec![],
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        pipeline.run().await; // Should panic
    }

    #[tokio::test]
    async fn pipeline_safety_check_cancelled() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects: vec![],
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        // Use force mode to bypass interactive prompts in test environment.
        // SafetyChecker prompt behavior is tested separately in safety module.
        config.force = true;
        let cancellation_token = create_pipeline_cancellation_token();

        // Cancel before running so the pipeline exits quickly
        cancellation_token.cancel();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        // Pipeline should complete without hanging
    }

    #[tokio::test]
    async fn pipeline_delete_all_versions_on_unversioned_bucket_clears_flag() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        // ListingMockStorage returns is_versioning_enabled() == false
        let storage: Storage = Box::new(ListingMockStorage {
            objects: vec![],
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.delete_all_versions = true;
        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        // Pipeline should succeed — the flag is silently cleared (Requirement 5.6)
        pipeline.run().await;
        assert!(!pipeline.has_error());
        assert!(!pipeline.config.delete_all_versions);
    }

    // -----------------------------------------------------------------------
    // Pipeline: max-delete threshold enforcement (Requirement 3.6)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_max_delete_cancels_when_threshold_exceeded() {
        init_dummy_tracing_subscriber();

        // Create 10 objects but set max_delete=3 — pipeline should stop after 3
        let mut objects = Vec::new();
        for i in 0..10 {
            objects.push(S3Object::NotVersioning(
                Object::builder()
                    .key(format!("file{i}.txt"))
                    .size(10)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ));
        }

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1; // Single deleter for predictable counting
        config.worker_size = 1; // Single worker for deterministic ordering
        config.max_delete = Some(3);

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning: has_warning.clone(),
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Pipeline should NOT have an error — max-delete is a cancellation, not an error
        assert!(!pipeline.has_error());

        // Should have a warning
        assert!(pipeline.has_warning());

        // Deleted count should be at most max_delete (3)
        let stats = pipeline.get_deletion_stats();
        assert!(
            stats.stats_deleted_objects <= 3,
            "Expected at most 3 deletions with max_delete=3, got {}",
            stats.stats_deleted_objects
        );
    }

    // -----------------------------------------------------------------------
    // Pipeline: event callback fires during execution (Requirements 7.6-7.7)
    // -----------------------------------------------------------------------

    /// Mock event callback that records all received events.
    struct RecordingEventCallback {
        events: Arc<Mutex<Vec<EventData>>>,
    }

    #[async_trait]
    impl crate::types::event_callback::EventCallback for RecordingEventCallback {
        async fn on_event(&mut self, event_data: EventData) {
            self.events.lock().unwrap().push(event_data);
        }
    }

    #[tokio::test]
    async fn pipeline_event_callback_fires_during_execution() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("event1.txt")
                    .size(50)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("event2.txt")
                    .size(75)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let recorded_events: Arc<Mutex<Vec<EventData>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = RecordingEventCallback {
            events: recorded_events.clone(),
        };

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.event_manager.register_callback(
            crate::types::event_callback::EventType::ALL_EVENTS,
            callback,
            false,
        );

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let events = recorded_events.lock().unwrap();

        // Should have received DELETE_COMPLETE events for each object
        let delete_complete_events: Vec<&EventData> = events
            .iter()
            .filter(|e| {
                e.event_type
                    .contains(crate::types::event_callback::EventType::DELETE_COMPLETE)
            })
            .collect();
        assert_eq!(
            delete_complete_events.len(),
            2,
            "Expected 2 DELETE_COMPLETE events, got {}",
            delete_complete_events.len()
        );

        // Verify event data includes object keys
        let keys: Vec<&str> = delete_complete_events
            .iter()
            .filter_map(|e| e.key.as_deref())
            .collect();
        assert!(keys.contains(&"event1.txt"), "Missing event1.txt in events");
        assert!(keys.contains(&"event2.txt"), "Missing event2.txt in events");

        // Should have received a PIPELINE_END event
        let end_events: Vec<&EventData> = events
            .iter()
            .filter(|e| {
                e.event_type
                    .contains(crate::types::event_callback::EventType::PIPELINE_END)
            })
            .collect();
        assert!(
            !end_events.is_empty(),
            "Expected at least one PIPELINE_END event"
        );
    }

    // -----------------------------------------------------------------------
    // Pipeline: Rust filter callback integration (Requirement 2.9)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_rust_filter_callback_integration() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("keep-me.log")
                    .size(100)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("remove-me.txt")
                    .size(200)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("also-keep.log")
                    .size(300)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        // Register a Rust filter callback that only accepts .log files
        struct LogOnlyFilter;

        #[async_trait]
        impl crate::types::filter_callback::FilterCallback for LogOnlyFilter {
            async fn filter(&mut self, object: &S3Object) -> Result<bool> {
                Ok(object.key().ends_with(".log"))
            }
        }

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.filter_manager.register_callback(LogOnlyFilter);
        config.test_user_defined_callback = true;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        // Only the two .log files should be deleted
        let stats = pipeline.get_deletion_stats();
        assert_eq!(
            stats.stats_deleted_objects, 2,
            "Expected 2 deletions (only .log files), got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(
            stats.stats_deleted_bytes, 400,
            "Expected 400 bytes (100 + 300), got {}",
            stats.stats_deleted_bytes
        );
    }

    // -----------------------------------------------------------------------
    // Pipeline: partial batch failure with fallback retry (Requirement 6.3)
    // -----------------------------------------------------------------------

    /// Mock storage where delete_objects returns partial failures
    /// but individual delete_object succeeds.
    #[derive(Clone)]
    struct PartialFailureMockStorage {
        objects: Vec<S3Object>,
        stats_sender: async_channel::Sender<DeletionStatistics>,
        has_warning: Arc<AtomicBool>,
        /// Keys that will fail in batch delete but succeed in single delete
        fail_in_batch: Arc<Vec<String>>,
    }

    #[async_trait]
    impl StorageTrait for PartialFailureMockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }

        async fn list_objects(
            &self,
            sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            for obj in &self.objects {
                sender.send(obj.clone()).await.unwrap();
            }
            Ok(())
        }

        async fn list_object_versions(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            Ok(())
        }

        async fn head_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
            Ok(aws_sdk_s3::operation::head_object::HeadObjectOutput::builder().build())
        }

        async fn get_object_tagging(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput> {
            Ok(
                aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput::builder()
                    .build()
                    .unwrap(),
            )
        }

        async fn delete_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> Result<aws_sdk_s3::operation::delete_object::DeleteObjectOutput> {
            // Individual deletes always succeed (the fallback retry works)
            Ok(aws_sdk_s3::operation::delete_object::DeleteObjectOutput::builder().build())
        }

        async fn delete_objects(
            &self,
            objects: Vec<aws_sdk_s3::types::ObjectIdentifier>,
        ) -> Result<aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput> {
            use aws_sdk_s3::types::{DeletedObject, Error as AwsS3Error};

            let mut builder = aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput::builder();

            for oi in &objects {
                let key = oi.key().to_string();
                if self.fail_in_batch.contains(&key) {
                    // Return a retryable error for this key
                    builder = builder.errors(
                        AwsS3Error::builder()
                            .key(&key)
                            .code("InternalError")
                            .message("Transient internal error")
                            .build(),
                    );
                } else {
                    builder = builder.deleted(
                        DeletedObject::builder()
                            .key(oi.key())
                            .set_version_id(oi.version_id().map(|s| s.to_string()))
                            .build(),
                    );
                }
            }

            Ok(builder.build())
        }

        async fn is_versioning_enabled(&self) -> Result<bool> {
            Ok(false)
        }

        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }

        fn get_stats_sender(&self) -> async_channel::Sender<DeletionStatistics> {
            self.stats_sender.clone()
        }

        async fn send_stats(&self, stats: DeletionStatistics) {
            let _ = self.stats_sender.send(stats).await;
        }

        fn set_warning(&self) {
            self.has_warning
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn pipeline_partial_batch_failure_retries_via_single_delete() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("ok1.txt")
                    .size(10)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("fail1.txt")
                    .size(20)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("ok2.txt")
                    .size(30)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("fail2.txt")
                    .size(40)
                    .last_modified(DateTime::from_secs(0))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let fail_in_batch = Arc::new(vec!["fail1.txt".to_string(), "fail2.txt".to_string()]);

        let storage: Storage = Box::new(PartialFailureMockStorage {
            objects: objects.clone(),
            stats_sender,
            has_warning: has_warning.clone(),
            fail_in_batch,
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1000; // Batch mode
        config.worker_size = 1;
        config.force_retry_config.force_retry_count = 1;
        config.force_retry_config.force_retry_interval_milliseconds = 0;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        // ALL 4 objects should be successfully deleted:
        // - ok1.txt and ok2.txt succeed in batch
        // - fail1.txt and fail2.txt fail in batch but succeed via single-delete fallback
        let stats = pipeline.get_deletion_stats();
        assert_eq!(
            stats.stats_deleted_objects, 4,
            "All 4 objects should be deleted (2 batch + 2 fallback), got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(
            stats.stats_deleted_bytes, 100,
            "Expected 100 bytes (10+20+30+40), got {}",
            stats.stats_deleted_bytes
        );
    }

    // -----------------------------------------------------------------------
    // Pipeline: error path — storage listing failure (Requirement 6)
    // -----------------------------------------------------------------------

    /// Mock storage that fails during listing.
    #[derive(Clone)]
    struct FailingListerStorage {
        stats_sender: async_channel::Sender<DeletionStatistics>,
        has_warning: Arc<AtomicBool>,
    }

    #[async_trait]
    impl StorageTrait for FailingListerStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }

        async fn list_objects(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            Err(anyhow::anyhow!("S3 ListObjects failed: AccessDenied"))
        }

        async fn list_object_versions(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            Err(anyhow::anyhow!("S3 ListObjectVersions failed"))
        }

        async fn head_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
            Ok(aws_sdk_s3::operation::head_object::HeadObjectOutput::builder().build())
        }

        async fn get_object_tagging(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput> {
            Ok(
                aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput::builder()
                    .build()
                    .unwrap(),
            )
        }

        async fn delete_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> Result<aws_sdk_s3::operation::delete_object::DeleteObjectOutput> {
            Ok(aws_sdk_s3::operation::delete_object::DeleteObjectOutput::builder().build())
        }

        async fn delete_objects(
            &self,
            _objects: Vec<aws_sdk_s3::types::ObjectIdentifier>,
        ) -> Result<aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput> {
            Ok(aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput::builder().build())
        }

        async fn is_versioning_enabled(&self) -> Result<bool> {
            Ok(false)
        }

        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }

        fn get_stats_sender(&self) -> async_channel::Sender<DeletionStatistics> {
            self.stats_sender.clone()
        }

        async fn send_stats(&self, stats: DeletionStatistics) {
            let _ = self.stats_sender.send(stats).await;
        }

        fn set_warning(&self) {
            self.has_warning
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn pipeline_listing_failure_produces_error() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(FailingListerStorage {
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Pipeline should report an error when listing fails
        assert!(
            pipeline.has_error(),
            "Pipeline must report error when listing fails"
        );
        let errors = pipeline.get_errors_and_consume().unwrap();
        assert!(
            !errors.is_empty(),
            "Error list must contain the listing failure"
        );
        let error_msg = errors[0].to_string();
        assert!(
            error_msg.contains("AccessDenied") || error_msg.contains("ListObjects"),
            "Error message should describe the listing failure, got: {}",
            error_msg
        );
    }

    // -----------------------------------------------------------------------
    // Pipeline: event callback fires on error path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_event_callback_fires_on_error() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(FailingListerStorage {
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let recorded_events: Arc<Mutex<Vec<EventData>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = RecordingEventCallback {
            events: recorded_events.clone(),
        };

        let mut config = create_test_config();
        config.force = true;
        config.event_manager.register_callback(
            crate::types::event_callback::EventType::ALL_EVENTS,
            callback,
            false,
        );

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(pipeline.has_error());

        let events = recorded_events.lock().unwrap();

        // Should have a PIPELINE_ERROR event
        let error_events: Vec<&EventData> = events
            .iter()
            .filter(|e| {
                e.event_type
                    .contains(crate::types::event_callback::EventType::PIPELINE_ERROR)
            })
            .collect();
        assert!(
            !error_events.is_empty(),
            "Expected PIPELINE_ERROR event on listing failure"
        );

        // Should always have PIPELINE_END
        let end_events: Vec<&EventData> = events
            .iter()
            .filter(|e| {
                e.event_type
                    .contains(crate::types::event_callback::EventType::PIPELINE_END)
            })
            .collect();
        assert!(
            !end_events.is_empty(),
            "Expected PIPELINE_END event even on error path"
        );
    }

    // -----------------------------------------------------------------------
    // Pipeline: warn_as_error feature (post-run promotion of warnings to errors)
    // -----------------------------------------------------------------------

    /// Mock storage where delete_objects returns non-retryable partial failures.
    /// Non-retryable errors stay as FailedKeys and trigger the warn_as_error path.
    #[derive(Clone)]
    struct NonRetryableFailureMockStorage {
        objects: Vec<S3Object>,
        stats_sender: async_channel::Sender<DeletionStatistics>,
        has_warning: Arc<AtomicBool>,
        /// Keys that will fail with non-retryable error in batch delete
        fail_in_batch: Arc<Vec<String>>,
    }

    #[async_trait]
    impl StorageTrait for NonRetryableFailureMockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }

        async fn list_objects(
            &self,
            sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            for obj in &self.objects {
                sender.send(obj.clone()).await.unwrap();
            }
            Ok(())
        }

        async fn list_object_versions(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            Ok(())
        }

        async fn head_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
            Ok(aws_sdk_s3::operation::head_object::HeadObjectOutput::builder().build())
        }

        async fn get_object_tagging(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> Result<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput> {
            Ok(
                aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput::builder()
                    .build()
                    .unwrap(),
            )
        }

        async fn delete_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> Result<aws_sdk_s3::operation::delete_object::DeleteObjectOutput> {
            Ok(aws_sdk_s3::operation::delete_object::DeleteObjectOutput::builder().build())
        }

        async fn delete_objects(
            &self,
            objects: Vec<aws_sdk_s3::types::ObjectIdentifier>,
        ) -> Result<aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput> {
            use aws_sdk_s3::types::{DeletedObject, Error as AwsS3Error};

            let mut builder = aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput::builder();

            for oi in &objects {
                let key = oi.key().to_string();
                if self.fail_in_batch.contains(&key) {
                    // Non-retryable error: AccessDenied
                    builder = builder.errors(
                        AwsS3Error::builder()
                            .key(&key)
                            .code("AccessDenied")
                            .message("Access denied")
                            .build(),
                    );
                } else {
                    builder = builder.deleted(
                        DeletedObject::builder()
                            .key(oi.key())
                            .set_version_id(oi.version_id().map(|s| s.to_string()))
                            .build(),
                    );
                }
            }

            Ok(builder.build())
        }

        async fn is_versioning_enabled(&self) -> Result<bool> {
            Ok(false)
        }

        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }

        fn get_stats_sender(&self) -> async_channel::Sender<DeletionStatistics> {
            self.stats_sender.clone()
        }

        async fn send_stats(&self, stats: DeletionStatistics) {
            let _ = self.stats_sender.send(stats).await;
        }

        fn set_warning(&self) {
            self.has_warning
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Pipeline integration test: warn_as_error=true with non-retryable
    /// partial failures should produce a pipeline error after completion.
    ///
    /// This tests the post-run check in execute_pipeline (line 267):
    /// if warn_as_error is true and has_warning is set, record a
    /// "warnings promoted to errors" error.
    #[tokio::test]
    async fn pipeline_warn_as_error_true_promotes_warnings_to_errors() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("ok.txt")
                    .size(10)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("fail.txt")
                    .size(20)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let fail_in_batch = Arc::new(vec!["fail.txt".to_string()]);

        let storage: Storage = Box::new(NonRetryableFailureMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
            fail_in_batch,
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1000;
        config.worker_size = 1;
        config.warn_as_error = true;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Pipeline MUST report an error because warn_as_error=true and failures occurred
        assert!(
            pipeline.has_error(),
            "Pipeline must report error when warn_as_error=true and warnings exist"
        );
        assert!(
            pipeline.has_warning(),
            "Warning flag must be set when partial failures occur"
        );

        let errors = pipeline.get_errors_and_consume().unwrap();
        let error_messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        assert!(
            error_messages
                .iter()
                .any(|m| m.contains("warnings promoted to errors")),
            "Error message should mention promotion, got: {:?}",
            error_messages
        );
    }

    /// Pipeline integration test: warn_as_error=false with non-retryable
    /// partial failures should produce a warning but NOT a pipeline error.
    #[tokio::test]
    async fn pipeline_warn_as_error_false_does_not_promote_warnings() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("ok.txt")
                    .size(10)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("fail.txt")
                    .size(20)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let fail_in_batch = Arc::new(vec!["fail.txt".to_string()]);

        let storage: Storage = Box::new(NonRetryableFailureMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
            fail_in_batch,
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1000;
        config.worker_size = 1;
        config.warn_as_error = false;

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Pipeline should NOT report an error when warn_as_error=false
        assert!(
            !pipeline.has_error(),
            "Pipeline should not report error when warn_as_error=false"
        );

        // But warning flag should still be set
        assert!(
            pipeline.has_warning(),
            "Warning flag should be set when partial failures occur"
        );

        // Some deletions should have succeeded
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 1); // "ok.txt"
        assert_eq!(stats.stats_failed_objects, 1); // "fail.txt"
    }

    // -------------------------------------------------------------------
    // Max-delete threshold through full pipeline
    // **Validates: Requirements 3.6**
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_max_delete_stops_at_threshold() {
        init_dummy_tracing_subscriber();

        // Create 10 objects, set max_delete=3
        let objects: Vec<S3Object> = (0..10)
            .map(|i| {
                S3Object::NotVersioning(
                    Object::builder()
                        .key(format!("obj/{i}"))
                        .size(50)
                        .last_modified(DateTime::from_secs(0))
                        .build(),
                )
            })
            .collect();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.max_delete = Some(3);

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        let stats = pipeline.get_deletion_stats();
        // max_delete=3, so at most 4 objects counted (3 + the triggering one)
        assert!(
            stats.stats_deleted_objects <= 4,
            "Expected at most 4 deleted objects (max_delete=3 + 1 triggering), got {}",
            stats.stats_deleted_objects
        );
        assert!(
            pipeline.has_warning(),
            "Warning flag should be set when max-delete threshold is exceeded"
        );
    }

    // -------------------------------------------------------------------
    // Listing failure always sets error flag
    // **Validates: Requirements 6.4, 6.5**
    // -------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10))]

        #[test]
        fn prop_listing_failure_always_sets_error(_seed in 0u32..50) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let (stats_sender, stats_receiver) = async_channel::unbounded();
                let has_warning = Arc::new(AtomicBool::new(false));
                let storage: Storage = Box::new(FailingListerStorage {
                    stats_sender,
                    has_warning: has_warning.clone(),
                });

                let mut config = create_test_config();
                config.force = true;

                let cancellation_token = create_pipeline_cancellation_token();

                let mut pipeline = DeletionPipeline {
                    config,
                    target: storage,
                    cancellation_token,
                    stats_receiver,
                    has_error: Arc::new(AtomicBool::new(false)),
                    has_panic: Arc::new(AtomicBool::new(false)),
                    has_warning,
                    errors: Arc::new(Mutex::new(VecDeque::new())),
                    ready: true,
                    prerequisites_checked: false,
                    deletion_stats_report: Arc::new(DeletionStatsReport::new()),
                };

                pipeline.run().await;

                prop_assert!(
                    pipeline.has_error(),
                    "Pipeline must set error flag when listing fails"
                );

                Ok(())
            })?;
        }
    }

    // -------------------------------------------------------------------
    // Accessor tests (has_panic, get_stats_receiver, get_error_messages)
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_has_panic_initially_false() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        assert!(!pipeline.has_panic());
    }

    #[tokio::test]
    async fn pipeline_has_panic_reflects_atomic_flag() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();
        let has_panic = Arc::new(AtomicBool::new(false));

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: has_panic.clone(),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        assert!(!pipeline.has_panic());
        has_panic.store(true, Ordering::SeqCst);
        assert!(pipeline.has_panic());
    }

    #[tokio::test]
    async fn pipeline_get_stats_receiver_returns_working_channel() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender.clone(), has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        let receiver = pipeline.get_stats_receiver();
        // Send through the sender and verify we can read from the cloned receiver
        let stats = DeletionStatistics::DeleteComplete {
            key: "test-key".to_string(),
        };
        stats_sender.send(stats).await.unwrap();
        let received = receiver.recv().await.unwrap();
        match received {
            DeletionStatistics::DeleteComplete { key } => assert_eq!(key, "test-key"),
            other => panic!("Expected DeleteComplete, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn pipeline_get_error_messages_none_when_no_errors() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        assert!(pipeline.get_error_messages().is_none());
    }

    #[tokio::test]
    async fn pipeline_get_error_messages_returns_messages() {
        init_dummy_tracing_subscriber();

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage = create_mock_storage(stats_sender, has_warning.clone());
        let config = create_test_config();
        let cancellation_token = create_pipeline_cancellation_token();

        let pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.record_error(anyhow::anyhow!("first error"));
        pipeline.record_error(anyhow::anyhow!("second error"));

        let messages = pipeline.get_error_messages().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "first error");
        assert_eq!(messages[1], "second error");
    }

    // -------------------------------------------------------------------
    // Prerequisites failure path (lines 131-138)
    // In test environment (no TTY), force=false & dry_run=false triggers
    // the non-interactive safety check which returns an error.
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_prerequisites_failure_records_error_and_stops() {
        init_dummy_tracing_subscriber();

        let objects = vec![S3Object::NotVersioning(
            Object::builder()
                .key("should-not-be-deleted.txt")
                .size(100)
                .last_modified(DateTime::from_secs(0))
                .build(),
        )];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        // No force, no dry_run, json_tracing → guaranteed non-interactive
        // (json_tracing forces is_non_interactive()=true regardless of TTY,
        // preventing the test from hanging on stdin in TTY environments)
        config.force = false;
        config.dry_run = false;
        config.tracing_config = Some(TracingConfig {
            tracing_level: log::Level::Info,
            json_tracing: true,
            aws_sdk_tracing: false,
            span_events_tracing: false,
            disable_color_tracing: false,
        });

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false, // Force prerequisites check
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;

        // Pipeline must have recorded an error
        assert!(
            pipeline.has_error(),
            "Pipeline must set error when prerequisites fail"
        );

        let errors = pipeline.get_errors_and_consume().unwrap();
        assert!(
            errors.iter().any(|e| e
                .to_string()
                .contains("Cannot run destructive operation without --force")),
            "Error should mention --force requirement, got: {:?}",
            errors.iter().map(|e| e.to_string()).collect::<Vec<_>>()
        );

        // No objects should have been deleted
        let stats = pipeline.get_deletion_stats();
        assert_eq!(stats.stats_deleted_objects, 0);
    }

    // -------------------------------------------------------------------
    // Filter branch coverage tests (lines 450-494)
    // Each test enables one filter in FilterConfig and verifies the
    // pipeline filters objects accordingly.
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_mtime_before_filter_applied() {
        init_dummy_tracing_subscriber();

        // Object at t=2000s, filter before_time=t=1500s → object is AFTER threshold → filtered out
        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("old.txt")
                    .size(10)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("new.txt")
                    .size(20)
                    .last_modified(DateTime::from_secs(2000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 1;
        // before_time = 1500s → only objects BEFORE this time pass
        config.filter_config.before_time = Some(chrono::DateTime::from_timestamp(1500, 0).unwrap());

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let stats = pipeline.get_deletion_stats();
        // Only "old.txt" (t=1000s) is before 1500s
        assert_eq!(
            stats.stats_deleted_objects, 1,
            "Only one object should pass mtime_before filter, got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(stats.stats_deleted_bytes, 10);
    }

    #[tokio::test]
    async fn pipeline_mtime_after_filter_applied() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("old.txt")
                    .size(10)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("new.txt")
                    .size(20)
                    .last_modified(DateTime::from_secs(2000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 1;
        // after_time = 1500s → only objects AFTER this time pass
        config.filter_config.after_time = Some(chrono::DateTime::from_timestamp(1500, 0).unwrap());

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let stats = pipeline.get_deletion_stats();
        // Only "new.txt" (t=2000s) is after 1500s
        assert_eq!(
            stats.stats_deleted_objects, 1,
            "Only one object should pass mtime_after filter, got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(stats.stats_deleted_bytes, 20);
    }

    #[tokio::test]
    async fn pipeline_smaller_size_filter_applied() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("small.txt")
                    .size(50)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("large.txt")
                    .size(500)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 1;
        // smaller_size = 100 → only objects SMALLER than 100 bytes pass
        config.filter_config.smaller_size = Some(100);

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let stats = pipeline.get_deletion_stats();
        assert_eq!(
            stats.stats_deleted_objects, 1,
            "Only small.txt should pass smaller_size filter, got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(stats.stats_deleted_bytes, 50);
    }

    #[tokio::test]
    async fn pipeline_larger_size_filter_applied() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("small.txt")
                    .size(50)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("large.txt")
                    .size(500)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 1;
        // larger_size = 100 → only objects LARGER than 100 bytes pass
        config.filter_config.larger_size = Some(100);

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let stats = pipeline.get_deletion_stats();
        assert_eq!(
            stats.stats_deleted_objects, 1,
            "Only large.txt should pass larger_size filter, got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(stats.stats_deleted_bytes, 500);
    }

    #[tokio::test]
    async fn pipeline_exclude_regex_filter_applied() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("keep.txt")
                    .size(10)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("remove.log")
                    .size(20)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 1;
        // exclude_regex = ".*\\.log$" → objects matching are filtered OUT
        config.filter_config.exclude_regex = Some(fancy_regex::Regex::new(r".*\.log$").unwrap());

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let stats = pipeline.get_deletion_stats();
        assert_eq!(
            stats.stats_deleted_objects, 1,
            "Only keep.txt should remain after exclude_regex, got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(stats.stats_deleted_bytes, 10);
    }

    // -------------------------------------------------------------------
    // Combined filter test — multiple filters at once
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_multiple_filters_combined() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            // small + old → passes smaller_size + mtime_before
            S3Object::NotVersioning(
                Object::builder()
                    .key("small-old.txt")
                    .size(50)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            // small + new → passes smaller_size but NOT mtime_before
            S3Object::NotVersioning(
                Object::builder()
                    .key("small-new.txt")
                    .size(50)
                    .last_modified(DateTime::from_secs(3000))
                    .build(),
            ),
            // large + old → passes mtime_before but NOT smaller_size
            S3Object::NotVersioning(
                Object::builder()
                    .key("large-old.txt")
                    .size(500)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
        ];

        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let storage: Storage = Box::new(ListingMockStorage {
            objects,
            stats_sender,
            has_warning: has_warning.clone(),
        });

        let mut config = create_test_config();
        config.force = true;
        config.batch_size = 1;
        config.worker_size = 1;
        // Both filters: smaller_size=100 AND before_time=2000s
        config.filter_config.smaller_size = Some(100);
        config.filter_config.before_time = Some(chrono::DateTime::from_timestamp(2000, 0).unwrap());

        let cancellation_token = create_pipeline_cancellation_token();

        let mut pipeline = DeletionPipeline {
            config,
            target: storage,
            cancellation_token,
            stats_receiver,
            has_error: Arc::new(AtomicBool::new(false)),
            has_panic: Arc::new(AtomicBool::new(false)),
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            prerequisites_checked: false,
            deletion_stats_report: Arc::new(DeletionStatsReport::new()),
        };

        pipeline.run().await;
        assert!(!pipeline.has_error());

        let stats = pipeline.get_deletion_stats();
        // Only "small-old.txt" passes both filters
        assert_eq!(
            stats.stats_deleted_objects, 1,
            "Only small-old.txt should pass both filters, got {}",
            stats.stats_deleted_objects
        );
        assert_eq!(stats.stats_deleted_bytes, 50);
    }
}
