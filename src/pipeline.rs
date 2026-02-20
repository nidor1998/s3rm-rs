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
/// # use s3rm_rs::config::Config;
/// # use s3rm_rs::types::token::create_pipeline_cancellation_token;
/// # use s3rm_rs::pipeline::DeletionPipeline;
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
    has_warning: Arc<AtomicBool>,
    errors: Arc<Mutex<VecDeque<anyhow::Error>>>,
    ready: bool,
    deletion_stats_report: Arc<Mutex<DeletionStatsReport>>,
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
        if let Err(e) = self.check_prerequisites().await {
            self.record_error(e);
            self.shutdown();
            self.fire_completion_events().await;
            return;
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
    fn get_error_messages(&self) -> Option<Vec<String>> {
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
        self.deletion_stats_report.lock().unwrap().snapshot()
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
    async fn check_prerequisites(&self) -> Result<()> {
        // Safety checks (confirmation prompt, dry-run, force flag)
        let checker = SafetyChecker::new(&self.config);
        checker.check_before_deletion()?;

        // Validate versioning prerequisite
        if self.config.delete_all_versions {
            let versioning_enabled = self.target.is_versioning_enabled().await?;
            if !versioning_enabled {
                return Err(anyhow::anyhow!(
                    "--delete-all-versions option requires versioning enabled on the target bucket."
                ));
            }
        }

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
            error!("terminator task panicked: {}", e);
            self.record_error(anyhow::anyhow!("terminator task panicked: {}", e));
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
            let mut event_data = EventData::new(EventType::PIPELINE_ERROR);
            event_data.message = Some(
                self.get_error_messages()
                    .unwrap_or_default()
                    .first()
                    .unwrap_or(&"Unknown error".to_string())
                    .to_string(),
            );
            self.config
                .event_manager
                .trigger_event(event_data.clone())
                .await;
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
        // Always insert this stage - it passes objects through if no callback registered.
        // UserDefinedFilter has its own filter() method (not ObjectFilter trait),
        // so we spawn it directly instead of via spawn_filter().
        {
            let (stage, new_receiver) =
                self.create_spsc_stage(Some(previous_stage_receiver), self.has_warning.clone());

            let has_error = self.has_error.clone();
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
    fn delete_objects(&self, filtered_objects: Receiver<S3Object>) -> Receiver<S3Object> {
        let (sender, next_stage_receiver) =
            async_channel::bounded::<S3Object>(self.config.object_listing_queue_size as usize);
        let delete_counter = Arc::new(AtomicU64::new(0));

        for worker_index in 0..self.config.worker_size {
            let stage = self.create_mpmc_stage(
                sender.clone(),
                filtered_objects.clone(),
                self.has_warning.clone(),
            );

            let mut object_deleter = ObjectDeleter::new(
                stage,
                worker_index,
                self.deletion_stats_report.clone(),
                delete_counter.clone(),
            );

            let has_error = self.has_error.clone();
            let error_list = self.errors.clone();
            let cancellation_token = self.cancellation_token.clone();
            let warn_as_error = self.config.warn_as_error;
            let has_warning = self.has_warning.clone();

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
                        error!(worker_index, "delete worker task panicked: {}", e);
                        error_list
                            .lock()
                            .unwrap()
                            .push_back(anyhow::anyhow!("delete worker panicked: {}", e));
                    }
                }

                // Promote warnings to errors if configured
                if warn_as_error && has_warning.load(Ordering::SeqCst) {
                    has_error.store(true, Ordering::SeqCst);
                    error_list.lock().unwrap().push_back(anyhow::anyhow!(
                        "warnings promoted to errors (--warn-as-error)"
                    ));
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
    use crate::filters::tests::{create_mock_storage, create_test_config};
    use crate::storage::StorageTrait;
    use crate::types::DeletionStatistics;
    use crate::types::token::create_pipeline_cancellation_token;
    use async_trait::async_trait;
    use aws_sdk_s3::types::Object;
    use std::sync::atomic::AtomicBool;

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            S3Object::NotVersioning(Object::builder().key("file1.txt").size(100).build()),
            S3Object::NotVersioning(Object::builder().key("file2.txt").size(200).build()),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
        };

        pipeline.run().await;

        // Pipeline should complete without errors in dry-run mode
        assert!(!pipeline.has_error());
    }

    #[tokio::test]
    async fn pipeline_runs_with_mock_storage_and_deletes() {
        init_dummy_tracing_subscriber();

        let objects = vec![S3Object::NotVersioning(
            Object::builder().key("delete-me.txt").size(50).build(),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            S3Object::NotVersioning(Object::builder().key("key1").size(10).build()),
            S3Object::NotVersioning(Object::builder().key("key2").size(20).build()),
            S3Object::NotVersioning(Object::builder().key("key3").size(30).build()),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
        };

        pipeline.run().await;

        // Should complete (either with some deletions or none, depending on timing)
        // The important thing is it doesn't hang
    }

    #[tokio::test]
    async fn pipeline_with_filters() {
        init_dummy_tracing_subscriber();

        let objects = vec![
            S3Object::NotVersioning(Object::builder().key("include-me.log").size(100).build()),
            S3Object::NotVersioning(Object::builder().key("exclude-me.txt").size(200).build()),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
        };

        pipeline.run().await;
        // Pipeline should complete without hanging
    }

    #[tokio::test]
    async fn pipeline_delete_all_versions_requires_versioning() {
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
            has_warning,
            errors: Arc::new(Mutex::new(VecDeque::new())),
            ready: true,
            deletion_stats_report: Arc::new(Mutex::new(DeletionStatsReport::new())),
        };

        pipeline.run().await;
        assert!(pipeline.has_error());
        let errors = pipeline.get_errors_and_consume().unwrap();
        assert!(
            errors[0]
                .to_string()
                .contains("--delete-all-versions option requires versioning enabled")
        );
    }
}
