//! Safety features for s3rm-rs deletion operations.
//!
//! Implements safeguards against accidental data loss:
//! - Dry-run mode: Returns DryRun error without performing deletions
//! - Confirmation prompts: Requires exact "yes" input
//! - Force flag: Skips confirmation prompts
//! - Max-delete threshold: Requires confirmation when count exceeds limit
//! - Non-TTY detection: Skips prompts in non-interactive environments
//! - JSON logging: Skips prompts to avoid corrupting structured output
//!
//! _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 13.1_

#[cfg(test)]
mod safety_properties;

use crate::config::Config;
use crate::types::error::S3rmError;
use anyhow::{Result, anyhow};
use std::io::{BufRead, IsTerminal, Write};

// ---------------------------------------------------------------------------
// DeletionSummary
// ---------------------------------------------------------------------------

/// Summary of objects to be deleted, displayed before confirmation.
///
/// Contains the total object count and estimated total size in bytes.
/// Displayed to the user before they confirm the deletion operation.
///
/// _Requirements: 3.5_
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionSummary {
    /// Total number of objects to be deleted.
    pub total_objects: u64,
    /// Estimated total size in bytes of objects to be deleted.
    pub total_bytes: u64,
}

impl DeletionSummary {
    pub fn new(total_objects: u64, total_bytes: u64) -> Self {
        Self {
            total_objects,
            total_bytes,
        }
    }
}

impl std::fmt::Display for DeletionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size_display = byte_unit::Byte::from_u64(self.total_bytes)
            .get_appropriate_unit(byte_unit::UnitType::Binary)
            .to_string();
        write!(
            f,
            "Objects to delete: {}, Estimated size: {}",
            self.total_objects, size_display
        )
    }
}

// ---------------------------------------------------------------------------
// PromptHandler trait (for testability)
// ---------------------------------------------------------------------------

/// Trait for handling user prompts, enabling testability.
///
/// The default implementation ([`StdioPromptHandler`]) uses stdin/stdout.
/// Tests can provide custom implementations to avoid blocking on user input.
pub trait PromptHandler: Send + Sync {
    /// Display the deletion summary and read a line of user input.
    ///
    /// If `threshold_exceeded` is true, an additional warning is displayed
    /// indicating the deletion count exceeds the `--max-delete` threshold.
    ///
    /// Returns the trimmed user input string.
    fn read_confirmation(
        &self,
        summary: &DeletionSummary,
        threshold_exceeded: bool,
    ) -> Result<String>;

    /// Check if the current environment supports interactive prompts.
    ///
    /// Returns `true` if both stdin and stdout are connected to a TTY.
    fn is_interactive(&self) -> bool;
}

/// Default prompt handler using stdin/stdout.
///
/// Uses `println!`/`print!` for prompts (not tracing) as specified
/// in the design document.
pub struct StdioPromptHandler;

impl PromptHandler for StdioPromptHandler {
    fn read_confirmation(
        &self,
        summary: &DeletionSummary,
        threshold_exceeded: bool,
    ) -> Result<String> {
        println!("\n{}", summary);
        if threshold_exceeded {
            println!("WARNING: Deletion count exceeds --max-delete threshold!");
        }
        print!("Type 'yes' to confirm deletion: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().lock().read_line(&mut input)?;
        Ok(input.trim().to_string())
    }

    fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
    }
}

// ---------------------------------------------------------------------------
// SafetyChecker
// ---------------------------------------------------------------------------

/// Safety checker that validates preconditions before deletion operations.
///
/// Orchestrates all safety checks in a defined order:
/// 1. Dry-run mode check (return DryRun error immediately)
/// 2. Force flag check (skip all prompts)
/// 3. Environment check (skip prompts if non-TTY or JSON logging)
/// 4. Max-delete threshold check
/// 5. User confirmation prompt (require exact "yes" input)
///
/// _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 13.1_
pub struct SafetyChecker {
    dry_run: bool,
    force: bool,
    json_logging: bool,
    max_delete: Option<u64>,
    prompt_handler: Box<dyn PromptHandler>,
}

impl SafetyChecker {
    /// Create a new SafetyChecker from the pipeline configuration.
    ///
    /// Uses [`StdioPromptHandler`] for interactive prompts.
    pub fn new(config: &Config) -> Self {
        let json_logging = config
            .tracing_config
            .map(|tc| tc.json_tracing)
            .unwrap_or(false);

        Self {
            dry_run: config.dry_run,
            force: config.force,
            json_logging,
            max_delete: config.max_delete,
            prompt_handler: Box::new(StdioPromptHandler),
        }
    }

    /// Create a SafetyChecker with a custom prompt handler (for testing).
    pub fn with_prompt_handler(config: &Config, prompt_handler: Box<dyn PromptHandler>) -> Self {
        let json_logging = config
            .tracing_config
            .map(|tc| tc.json_tracing)
            .unwrap_or(false);

        Self {
            dry_run: config.dry_run,
            force: config.force,
            json_logging,
            max_delete: config.max_delete,
            prompt_handler,
        }
    }

    /// Check all safety preconditions before starting a deletion operation.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if deletion should proceed
    /// - `Err(S3rmError::DryRun)` if dry-run mode is enabled
    /// - `Err(S3rmError::Cancelled)` if the user declines confirmation
    ///
    /// # Decision Flow
    ///
    /// 1. If `dry_run` is true → return `Err(S3rmError::DryRun)`
    /// 2. If `force` is true → return `Ok(())`
    /// 3. If non-interactive (non-TTY or JSON logging) → return `Ok(())`
    /// 4. Display summary and prompt for confirmation
    /// 5. Accept only exact "yes" input; anything else → `Err(S3rmError::Cancelled)`
    ///
    /// _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 13.1_
    pub fn check_before_deletion(&self, summary: &DeletionSummary) -> Result<()> {
        // 1. Dry-run mode: return early without any deletions
        if self.dry_run {
            return Err(anyhow!(S3rmError::DryRun));
        }

        // 2. Force flag: skip all prompts
        if self.force {
            return Ok(());
        }

        // 3. Check if prompts should be skipped (non-interactive environment)
        if self.should_skip_prompt() {
            return Ok(());
        }

        // 4. Check max-delete threshold
        let threshold_exceeded = self.is_threshold_exceeded(summary.total_objects);

        // 5. Prompt for confirmation
        self.prompt_confirmation(summary, threshold_exceeded)
    }

    /// Check if the deletion count exceeds the max-delete threshold.
    ///
    /// Returns `true` if `max_delete` is set and `object_count` exceeds it.
    ///
    /// _Requirements: 3.6_
    pub fn is_threshold_exceeded(&self, object_count: u64) -> bool {
        match self.max_delete {
            Some(max) => object_count > max,
            None => false,
        }
    }

    /// Determine if prompts should be skipped due to environment conditions.
    ///
    /// Prompts are skipped when:
    /// - JSON logging is enabled (would corrupt structured output)
    /// - The environment is non-interactive (no TTY on stdin/stdout)
    ///
    /// _Requirements: 13.1_
    fn should_skip_prompt(&self) -> bool {
        // Skip if JSON logging is enabled (would corrupt structured output)
        if self.json_logging {
            return true;
        }

        // Skip in non-interactive (non-TTY) environments
        if !self.prompt_handler.is_interactive() {
            return true;
        }

        false
    }

    /// Prompt the user for confirmation and validate their response.
    ///
    /// Displays the deletion summary and optionally a threshold warning,
    /// then requires the user to type exactly "yes" to proceed.
    ///
    /// _Requirements: 3.2, 3.3_
    fn prompt_confirmation(
        &self,
        summary: &DeletionSummary,
        threshold_exceeded: bool,
    ) -> Result<()> {
        let input = self
            .prompt_handler
            .read_confirmation(summary, threshold_exceeded)?;

        if input != "yes" {
            return Err(anyhow!(S3rmError::Cancelled));
        }

        Ok(())
    }
}
