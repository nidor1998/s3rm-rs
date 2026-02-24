//! Safety features for s3rm-rs deletion operations.
//!
//! Implements safeguards against accidental data loss:
//! - Dry-run mode: Skips confirmation (pipeline runs but deletions are simulated)
//! - Confirmation prompts: Requires exact "yes" input for destructive operations
//! - Force flag: Skips confirmation prompts
//! - Max-delete threshold: Enforced at deletion time in ObjectDeleter
//! - Non-TTY detection: Errors when no TTY and --force is not set
//! - JSON logging: Errors when JSON logging and --force is not set
//!
//! Note: Object count and size estimation is handled by dry-run mode.
//! The confirmation prompt shows the target prefix to make clear what
//! will be deleted.
//!
//! _Requirements: 3.1, 3.2, 3.3, 3.4, 3.6, 13.1_

#[cfg(test)]
mod safety_properties;

use crate::config::Config;
use crate::types::StoragePath;
use crate::types::error::S3rmError;
use anyhow::{Result, anyhow};
use std::io::{BufRead, IsTerminal, Write};

// ---------------------------------------------------------------------------
// ANSI color helpers
// ---------------------------------------------------------------------------

const ANSI_BOLD_RED: &str = "\x1b[1;31m";
const ANSI_BOLD_YELLOW: &str = "\x1b[1;33m";
const ANSI_RESET: &str = "\x1b[0m";

// ---------------------------------------------------------------------------
// PromptHandler trait (for testability)
// ---------------------------------------------------------------------------

/// Trait for handling user prompts, enabling testability.
///
/// The default implementation ([`StdioPromptHandler`]) uses stdin/stdout.
/// Tests can provide custom implementations to avoid blocking on user input.
pub trait PromptHandler: Send + Sync {
    /// Display the confirmation prompt and read a line of user input.
    ///
    /// `target_display` is the human-readable target (e.g. `s3://bucket/prefix`).
    /// `use_color` controls whether ANSI color codes are emitted.
    ///
    /// Returns the trimmed user input string.
    fn read_confirmation(&self, target_display: &str, use_color: bool) -> Result<String>;

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
    fn read_confirmation(&self, target_display: &str, use_color: bool) -> Result<String> {
        if use_color {
            println!(
                "\n{}WARNING:{} All objects matching prefix {}{}{}  will be deleted.",
                ANSI_BOLD_RED, ANSI_RESET, ANSI_BOLD_YELLOW, target_display, ANSI_RESET,
            );
        } else {
            println!(
                "\nWARNING: All objects matching prefix {}  will be deleted.",
                target_display,
            );
        }
        println!(
            "Use --dry-run to preview which objects would be deleted without actually removing them.\n"
        );
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
/// 1. Dry-run mode check (skip confirmation — pipeline runs but deletions are simulated)
/// 2. Force flag check (skip all prompts)
/// 3. Environment check (error if non-TTY or JSON logging without --force)
/// 4. User confirmation prompt (require exact "yes" input)
///
/// Note: Dry-run mode does NOT abort the pipeline. The pipeline runs fully
/// (listing, filtering) but the deletion layer simulates successful deletions
/// and outputs statistics. The SafetyChecker simply skips the confirmation
/// prompt since no destructive operation will occur.
///
/// _Requirements: 3.1, 3.2, 3.3, 3.4, 13.1_
pub struct SafetyChecker {
    dry_run: bool,
    force: bool,
    json_logging: bool,
    disable_color: bool,
    target_display: String,
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

        let disable_color = config
            .tracing_config
            .map(|tc| tc.disable_color_tracing)
            .unwrap_or(false);

        Self {
            dry_run: config.dry_run,
            force: config.force,
            json_logging,
            disable_color,
            target_display: format_target(&config.target),
            prompt_handler: Box::new(StdioPromptHandler),
        }
    }

    /// Create a SafetyChecker with a custom prompt handler (for testing).
    #[cfg(test)]
    pub fn with_prompt_handler(config: &Config, prompt_handler: Box<dyn PromptHandler>) -> Self {
        let json_logging = config
            .tracing_config
            .map(|tc| tc.json_tracing)
            .unwrap_or(false);

        let disable_color = config
            .tracing_config
            .map(|tc| tc.disable_color_tracing)
            .unwrap_or(false);

        Self {
            dry_run: config.dry_run,
            force: config.force,
            json_logging,
            disable_color,
            target_display: format_target(&config.target),
            prompt_handler,
        }
    }

    /// Check all safety preconditions before starting a deletion operation.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the pipeline should proceed
    /// - `Err(S3rmError::Cancelled)` if the user declines confirmation
    ///
    /// # Decision Flow
    ///
    /// 1. If `dry_run` is true → return `Ok(())` (no confirmation needed;
    ///    the pipeline runs but the deletion layer simulates deletions)
    /// 2. If `force` is true → return `Ok(())`
    /// 3. If non-interactive (non-TTY or JSON logging) → return `Err`
    ///    (unsafe to proceed without confirmation and without `--force`)
    /// 4. Prompt for confirmation (require exact "yes" input)
    ///
    /// Note: Object count/size estimation is handled by dry-run mode.
    /// The max-delete threshold is enforced at deletion time in ObjectDeleter.
    ///
    /// _Requirements: 3.1, 3.2, 3.3, 3.4, 13.1_
    pub fn check_before_deletion(&self) -> Result<()> {
        // 1. Dry-run mode: skip confirmation — the pipeline will run but
        //    the deletion layer simulates successful deletions and outputs stats.
        if self.dry_run {
            return Ok(());
        }

        // 2. Force flag: skip all prompts
        if self.force {
            return Ok(());
        }

        // 3. Non-interactive environment: error out (unsafe without --force)
        if self.is_non_interactive() {
            return Err(anyhow!(S3rmError::InvalidConfig(
                "Cannot run destructive operation without --force (-f) in a non-interactive environment (no TTY). \
                 Use --force to skip confirmation, or --dry-run to preview deletions."
                    .to_string()
            )));
        }

        // 4. Prompt for confirmation
        self.prompt_confirmation()
    }

    /// Check if the environment is non-interactive (cannot prompt the user).
    ///
    /// Returns `true` when:
    /// - JSON logging is enabled (prompts would corrupt structured output)
    /// - The environment has no TTY on stdin/stdout
    ///
    /// _Requirements: 13.1_
    fn is_non_interactive(&self) -> bool {
        // JSON logging: prompts would corrupt structured output
        if self.json_logging {
            return true;
        }

        // No TTY on stdin/stdout
        if !self.prompt_handler.is_interactive() {
            return true;
        }

        false
    }

    /// Prompt the user for confirmation and validate their response.
    ///
    /// Displays a warning showing the target prefix that will be deleted,
    /// then requires the user to type exactly "yes" to proceed.
    ///
    /// _Requirements: 3.2, 3.3_
    fn prompt_confirmation(&self) -> Result<()> {
        let use_color = !self.disable_color;
        let input = self
            .prompt_handler
            .read_confirmation(&self.target_display, use_color)?;

        if input != "yes" {
            return Err(anyhow!(S3rmError::Cancelled));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format the storage target as an S3 URI for display.
fn format_target(target: &StoragePath) -> String {
    match target {
        StoragePath::S3 { bucket, prefix } => {
            if prefix.is_empty() {
                format!("s3://{}/", bucket)
            } else {
                format!("s3://{}/{}", bucket, prefix)
            }
        }
    }
}
