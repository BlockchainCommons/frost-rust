//! Busy indicators for get/put operations.
//!
//! Provides visual feedback during Hubert storage operations with:
//! - ⬇️ prefix for get (download) operations
//! - ⬆️ prefix for put (upload) operations
//! - 🔄 animated spinner while in progress
//! - ✅ on success, ❌ on failure
//! - Countdown timer for get operations (time remaining)
//! - Count-up timer for put operations (elapsed time)

use std::{
    io::IsTerminal,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use bc_components::ARID;
use bc_envelope::Envelope;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::runtime::Runtime;

use crate::cmd::storage::StorageClient;

/// Direction of the operation (get or put).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Downloading from storage (⬇️)
    Get,
    /// Uploading to storage (⬆️)
    Put,
}

impl Direction {
    /// Get the emoji prefix for this direction.
    pub fn emoji(&self) -> &'static str {
        match self {
            Direction::Get => "⬇️",
            Direction::Put => "⬆️",
        }
    }
}

/// A busy indicator for a single operation.
pub struct BusyIndicator {
    direction: Direction,
    name: String,
    bar: Option<ProgressBar>,
    start_time: Instant,
    timeout_seconds: Option<u64>,
    is_interactive: bool,
    stop_flag: Arc<AtomicBool>,
}

impl BusyIndicator {
    /// Create a new busy indicator.
    ///
    /// # Arguments
    /// * `direction` - Whether this is a get or put operation
    /// * `name` - Display name (e.g., participant name)
    /// * `timeout_seconds` - For get operations, the countdown timeout
    pub fn new(
        direction: Direction,
        name: impl Into<String>,
        timeout_seconds: Option<u64>,
    ) -> Self {
        let name = name.into();
        let is_interactive = std::io::stderr().is_terminal();
        let stop_flag = Arc::new(AtomicBool::new(false));

        let bar = if is_interactive {
            let bar = ProgressBar::new_spinner();
            let template = match direction {
                Direction::Get => {
                    if let Some(timeout) = timeout_seconds {
                        format!(
                            "{}  {{spinner:.yellow}} {}... {}s",
                            direction.emoji(),
                            name,
                            timeout
                        )
                    } else {
                        format!(
                            "{}  {{spinner:.yellow}} {}...",
                            direction.emoji(),
                            name
                        )
                    }
                }
                Direction::Put => {
                    format!(
                        "{}  {{spinner:.yellow}} {}... 0s",
                        direction.emoji(),
                        name
                    )
                }
            };
            bar.set_style(
                ProgressStyle::default_spinner()
                    .template(&template)
                    .expect("valid template"),
            );
            bar.enable_steady_tick(Duration::from_millis(100));
            Some(bar)
        } else {
            // Non-interactive: no incremental output, we print final status
            // only
            None
        };

        Self {
            direction,
            name,
            bar,
            start_time: Instant::now(),
            timeout_seconds,
            is_interactive,
            stop_flag,
        }
    }

    /// Start the timer update loop (call this in a separate thread).
    pub fn start_timer_updates(&self) {
        if !self.is_interactive {
            return;
        }

        let bar = match &self.bar {
            Some(b) => b.clone(),
            None => return,
        };
        let direction = self.direction;
        let name = self.name.clone();
        let timeout = self.timeout_seconds;
        let stop_flag = Arc::clone(&self.stop_flag);
        let start = self.start_time;

        std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                let elapsed = start.elapsed().as_secs();

                let template = match direction {
                    Direction::Get => {
                        if let Some(t) = timeout {
                            let remaining = t.saturating_sub(elapsed);
                            format!(
                                "{}  {{spinner:.yellow}} {}... {}s",
                                direction.emoji(),
                                name,
                                remaining
                            )
                        } else {
                            format!(
                                "{}  {{spinner:.yellow}} {}...",
                                direction.emoji(),
                                name
                            )
                        }
                    }
                    Direction::Put => {
                        format!(
                            "{}  {{spinner:.yellow}} {}... {}s",
                            direction.emoji(),
                            name,
                            elapsed
                        )
                    }
                };
                bar.set_style(
                    ProgressStyle::default_spinner()
                        .template(&template)
                        .expect("valid template"),
                );
            }
        });
    }

    /// Mark the operation as successful.
    pub fn success(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs();

        if self.is_interactive {
            if let Some(ref bar) = self.bar {
                // Both get and put show elapsed time on success
                let template = format!(
                    "{}  ✅ {}: {}s",
                    self.direction.emoji(),
                    self.name,
                    elapsed
                );
                bar.set_style(
                    ProgressStyle::default_spinner()
                        .template(&template)
                        .expect("valid template"),
                );
                bar.finish();
                // Ensure we're on a new line after finish
                eprintln!();
            }
        } else {
            // Non-interactive: print complete status line
            eprintln!(
                "{}  ✅ {}: {}s",
                self.direction.emoji(),
                self.name,
                elapsed
            );
        }
    }

    /// Mark the operation as failed with an error message.
    pub fn error(&self, msg: &str) {
        self.stop_flag.store(true, Ordering::Relaxed);

        if self.is_interactive {
            if let Some(ref bar) = self.bar {
                let template = format!(
                    "{}  ❌ {}: {}",
                    self.direction.emoji(),
                    self.name,
                    msg
                );
                bar.set_style(
                    ProgressStyle::default_spinner()
                        .template(&template)
                        .expect("valid template"),
                );
                bar.finish();
                // Ensure we're on a new line after finish
                eprintln!();
            }
        } else {
            // Non-interactive: print complete status line
            eprintln!("{}  ❌ {}: {}", self.direction.emoji(), self.name, msg);
        }
    }

    /// Mark the operation as timed out.
    pub fn timeout(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);

        if self.is_interactive {
            if let Some(ref bar) = self.bar {
                let template = format!(
                    "{}  ❌ {}: Timeout",
                    self.direction.emoji(),
                    self.name
                );
                bar.set_style(
                    ProgressStyle::default_spinner()
                        .template(&template)
                        .expect("valid template"),
                );
                bar.finish();
                // Ensure we're on a new line after finish
                eprintln!();
            }
        } else {
            // Non-interactive: print complete status line
            eprintln!("{}  ❌ {}: Timeout", self.direction.emoji(), self.name);
        }
    }
}

impl Drop for BusyIndicator {
    fn drop(&mut self) { self.stop_flag.store(true, Ordering::Relaxed); }
}

/// Execute a get operation with busy indicator.
///
/// Shows progress while fetching from Hubert storage.
pub fn get_with_indicator(
    runtime: &Runtime,
    client: &StorageClient,
    arid: &ARID,
    name: impl Into<String>,
    timeout: Option<u64>,
) -> Result<Option<Envelope>> {
    let indicator = BusyIndicator::new(Direction::Get, name, timeout);
    indicator.start_timer_updates();

    let result = runtime.block_on(async { client.get(arid, timeout).await });

    match &result {
        Ok(Some(_)) => indicator.success(),
        Ok(None) => indicator.error("Not found"),
        Err(e) => {
            let msg = e.to_string();
            if msg.to_lowercase().contains("timeout") {
                indicator.timeout();
            } else {
                indicator.error(&msg);
            }
        }
    }

    result
}

/// Execute a put operation with busy indicator.
///
/// Shows progress while uploading to Hubert storage.
pub fn put_with_indicator(
    runtime: &Runtime,
    client: &StorageClient,
    arid: &ARID,
    envelope: &Envelope,
    name: impl Into<String>,
) -> Result<String> {
    let indicator = BusyIndicator::new(Direction::Put, name, None);
    indicator.start_timer_updates();

    let result = runtime.block_on(async { client.put(arid, envelope).await });

    match &result {
        Ok(_) => indicator.success(),
        Err(e) => indicator.error(&e.to_string()),
    }

    result
}

/// Check if the terminal is interactive.
pub fn is_interactive() -> bool { std::io::stderr().is_terminal() }
