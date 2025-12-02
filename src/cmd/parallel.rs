//! Parallel fetch and send utilities for coordinator commands.
//!
//! This module provides utilities for fetching responses from multiple
//! participants in parallel with interactive progress display.
//!
//! # Threading Model
//!
//! The Hubert `KvStore` trait uses `#[async_trait(?Send)]`, meaning its
//! futures are not `Send` and cannot be spawned across threads. Instead,
//! we use `tokio::task::LocalSet` to run concurrent tasks on the same thread.
//!
//! # Visual Indicators
//!
//! - ⬇️ prefix for get (download) operations with countdown timer
//! - ⬆️ prefix for put (upload) operations with count-up timer
//! - 🔄 animated spinner while in progress
//! - ✅ on success, ❌ on failure

use std::{
    collections::HashMap,
    io::IsTerminal,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use bc_components::{ARID, XID};
use bc_envelope::Envelope;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::{sync::Mutex, time::Instant};

use crate::cmd::storage::StorageClient;

/// Status of a participant's response fetch.
#[derive(Debug, Clone)]
pub enum FetchStatus {
    /// Waiting for response
    Pending,
    /// Response received and validated
    Success(Envelope),
    /// Participant explicitly rejected the request
    Rejected(String),
    /// Network or parsing error
    Error(String),
    /// No response within timeout
    Timeout,
}

/// Configuration for parallel fetch operations.
#[derive(Debug, Clone)]
pub struct ParallelFetchConfig {
    /// Maximum time to wait for all responses (in seconds)
    pub timeout_seconds: Option<u64>,
}

impl Default for ParallelFetchConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: Some(600), // 10 minutes default
        }
    }
}

impl ParallelFetchConfig {
    /// Create a new config with the specified timeout.
    pub fn with_timeout(timeout_seconds: Option<u64>) -> Self {
        Self { timeout_seconds }
    }
}

/// Result of collecting responses from multiple participants.
#[derive(Debug)]
pub struct CollectionResult<T> {
    /// Successful responses
    pub successes: Vec<(XID, T)>,
    /// Participants who explicitly rejected
    pub rejections: Vec<(XID, String)>,
    /// Participants with network/parsing errors
    pub errors: Vec<(XID, String)>,
    /// Participants who timed out
    pub timeouts: Vec<XID>,
}

impl<T> CollectionResult<T> {
    /// Check if enough responses were received to proceed.
    pub fn can_proceed(&self, min_required: usize) -> bool {
        self.successes.len() >= min_required
    }

    /// Total number of participants
    pub fn total(&self) -> usize {
        self.successes.len()
            + self.rejections.len()
            + self.errors.len()
            + self.timeouts.len()
    }

    /// Check if all responses succeeded
    pub fn all_succeeded(&self) -> bool {
        self.rejections.is_empty()
            && self.errors.is_empty()
            && self.timeouts.is_empty()
    }
}

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

/// Progress display for parallel operations.
pub struct ProgressDisplay {
    #[allow(dead_code)]
    multi: MultiProgress,
    bars: HashMap<XID, (ProgressBar, String)>,
    start_time: Instant,
    timeout_seconds: u64,
    direction: Direction,
    stop_flag: Arc<AtomicBool>,
    elapsed_tracker: Arc<AtomicU64>,
}

impl ProgressDisplay {
    /// Create a new progress display for get operations (with countdown).
    pub fn new_get(
        participants: &[(XID, String)],
        timeout_seconds: u64,
    ) -> Self {
        Self::new_internal(participants, timeout_seconds, Direction::Get)
    }

    /// Create a new progress display for put operations (with count-up).
    pub fn new_put(participants: &[(XID, String)]) -> Self {
        Self::new_internal(participants, 60, Direction::Put)
    }

    /// Create a new progress display for the given participants.
    pub fn new(participants: &[(XID, String)], timeout_seconds: u64) -> Self {
        // Default to Get for backward compatibility
        Self::new_internal(participants, timeout_seconds, Direction::Get)
    }

    fn new_internal(
        participants: &[(XID, String)],
        timeout_seconds: u64,
        direction: Direction,
    ) -> Self {
        let multi = MultiProgress::new();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let elapsed_tracker = Arc::new(AtomicU64::new(0));
        let start_time = Instant::now();

        let mut bars = HashMap::new();
        for (xid, name) in participants {
            let bar = multi.add(ProgressBar::new_spinner());
            let template = match direction {
                Direction::Get => {
                    format!(
                        "{}  {{spinner:.yellow}} {}... -{}s",
                        direction.emoji(),
                        name,
                        timeout_seconds
                    )
                }
                Direction::Put => {
                    format!(
                        "{}  {{spinner:.yellow}} {}... +0s",
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
            bars.insert(*xid, (bar, name.clone()));
        }

        Self {
            multi,
            bars,
            start_time,
            timeout_seconds,
            direction,
            stop_flag,
            elapsed_tracker,
        }
    }

    /// Start a background timer update thread.
    pub fn start_timer_updates(&self) {
        let bars: Vec<_> = self
            .bars
            .iter()
            .map(|(xid, (bar, name))| (*xid, bar.clone(), name.clone()))
            .collect();
        let timeout = self.timeout_seconds;
        let direction = self.direction;
        let stop_flag = Arc::clone(&self.stop_flag);
        let elapsed_tracker = Arc::clone(&self.elapsed_tracker);
        let start = self.start_time;

        std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                let elapsed = start.elapsed().as_secs();
                elapsed_tracker.store(elapsed, Ordering::Relaxed);

                // Update individual bars that are still pending
                for (_xid, bar, name) in &bars {
                    if !bar.is_finished() {
                        let template = match direction {
                            Direction::Get => {
                                let remaining = timeout.saturating_sub(elapsed);
                                format!(
                                    "{}  {{spinner:.yellow}} {}... -{}s",
                                    direction.emoji(),
                                    name,
                                    remaining
                                )
                            }
                            Direction::Put => {
                                format!(
                                    "{}  {{spinner:.yellow}} {}... +{}s",
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
                }
            }
        });
    }

    /// Get the current elapsed time in seconds.
    pub fn elapsed_seconds(&self) -> u64 { self.start_time.elapsed().as_secs() }

    /// Mark a participant as successful.
    pub fn mark_success(&self, xid: &XID) {
        if let Some((bar, name)) = self.bars.get(xid) {
            let elapsed = self.elapsed_seconds();
            // Both get and put show elapsed time on success
            let template = format!(
                "{}  ✅ {}: {}s",
                self.direction.emoji(),
                name,
                elapsed
            );
            bar.set_style(
                ProgressStyle::default_spinner()
                    .template(&template)
                    .expect("valid template"),
            );
            bar.finish();
        }
    }

    /// Mark a participant as failed with an error message.
    pub fn mark_error(&self, xid: &XID, error: &str) {
        if let Some((bar, name)) = self.bars.get(xid) {
            let template =
                format!("{}  ❌ {}: {}", self.direction.emoji(), name, error);
            bar.set_style(
                ProgressStyle::default_spinner()
                    .template(&template)
                    .expect("valid template"),
            );
            bar.finish();
        }
    }

    /// Mark a participant as timed out.
    pub fn mark_timeout(&self, xid: &XID) {
        if let Some((bar, name)) = self.bars.get(xid) {
            let template =
                format!("{}  ❌ {}: Timeout", self.direction.emoji(), name);
            bar.set_style(
                ProgressStyle::default_spinner()
                    .template(&template)
                    .expect("valid template"),
            );
            bar.finish();
        }
    }

    /// Finish all progress bars.
    pub fn finish(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        for (bar, _) in self.bars.values() {
            bar.finish();
        }
        // Ensure we're on a new line after all progress bars finish
        eprintln!();
    }

    /// Clear all progress bars without marking complete.
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        for (bar, _) in self.bars.values() {
            bar.finish_and_clear();
        }
    }
}

/// Streaming output for non-interactive terminals.
pub struct StreamingOutput {
    direction: Direction,
    #[allow(dead_code)]
    verbose: bool,
}

impl StreamingOutput {
    /// Create a new streaming output for get operations.
    pub fn new_get(verbose: bool) -> Self {
        Self { direction: Direction::Get, verbose }
    }

    /// Create a new streaming output for put operations.
    pub fn new_put(verbose: bool) -> Self {
        Self { direction: Direction::Put, verbose }
    }

    /// Create a new streaming output (defaults to Get for backward
    /// compatibility).
    pub fn new(verbose: bool) -> Self { Self::new_get(verbose) }

    /// Print that we're starting to wait.
    pub fn started(&self, count: usize) {
        match self.direction {
            Direction::Get => {
                eprintln!("Waiting for {} responses...", count);
            }
            Direction::Put => {
                eprintln!("Sending to {} participants...", count);
            }
        }
    }

    /// Print a success message.
    pub fn success(&self, name: &str, elapsed_secs: Option<u64>) {
        // Both get and put show elapsed time if available
        if let Some(secs) = elapsed_secs {
            eprintln!("{}  ✅ {}: {}s", self.direction.emoji(), name, secs);
        } else {
            eprintln!("{}  ✅ {}", self.direction.emoji(), name);
        }
    }

    /// Print an error message.
    pub fn error(&self, name: &str, error: &str) {
        eprintln!("{}  ❌ {}: {}", self.direction.emoji(), name, error);
    }

    /// Print a timeout message.
    pub fn timeout(&self, name: &str) {
        eprintln!("{}  ❌ {}: Timeout", self.direction.emoji(), name);
    }
}

/// Check if stderr is an interactive terminal.
pub fn is_interactive_terminal() -> bool { std::io::stderr().is_terminal() }

/// Fetch responses from multiple participants in parallel with progress
/// display.
///
/// Uses `tokio::task::LocalSet` because Hubert's `KvStore` futures are `!Send`.
///
/// # Arguments
///
/// * `client` - The storage client to use for fetching
/// * `requests` - List of (participant_xid, arid, display_name) tuples
/// * `config` - Configuration including timeout
/// * `validate` - Closure to validate and extract data from each envelope
///
/// # Returns
///
/// A `CollectionResult` containing categorized results from all participants.
pub async fn parallel_fetch<F, T>(
    client: Arc<StorageClient>,
    requests: Vec<(XID, ARID, String)>,
    config: ParallelFetchConfig,
    validate: F,
) -> Result<CollectionResult<T>>
where
    F: Fn(&Envelope, &XID) -> Result<T> + Clone + 'static,
    T: 'static,
{
    let timeout_secs = config.timeout_seconds.unwrap_or(600);
    let is_interactive = is_interactive_terminal();
    let participant_count = requests.len();

    // Set up progress display or streaming output
    let progress = if is_interactive {
        let p = Arc::new(ProgressDisplay::new_get(
            &requests
                .iter()
                .map(|(xid, _, name)| (*xid, name.clone()))
                .collect::<Vec<_>>(),
            timeout_secs,
        ));
        p.start_timer_updates();
        Some(p)
    } else {
        None
    };

    let streaming = if !is_interactive {
        let s = StreamingOutput::new_get(true);
        s.started(participant_count);
        Some(Arc::new(s))
    } else {
        None
    };

    // Shared results collection
    #[allow(clippy::type_complexity)]
    let results: Arc<Mutex<Vec<(XID, String, Result<T>)>>> =
        Arc::new(Mutex::new(Vec::new()));

    // Use LocalSet for !Send futures
    let local_set = tokio::task::LocalSet::new();

    local_set
        .run_until(async {
            // Spawn all fetch tasks
            let mut handles = Vec::new();
            for (xid, arid, name) in requests {
                let client = Arc::clone(&client);
                let validate = validate.clone();
                let results = Arc::clone(&results);
                let progress = progress.clone();
                let streaming = streaming.clone();
                let timeout = timeout_secs;

                let handle = tokio::task::spawn_local(async move {
                    let fetch_result = tokio::time::timeout(
                        Duration::from_secs(timeout),
                        client.get(&arid, Some(timeout)),
                    )
                    .await;

                    let result = match fetch_result {
                        Ok(Ok(Some(env))) => validate(&env, &xid),
                        Ok(Ok(None)) => Err(anyhow::anyhow!("Not found")),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(anyhow::anyhow!("Timeout")),
                    };

                    // Update display
                    if let Some(ref p) = progress {
                        match &result {
                            Ok(_) => p.mark_success(&xid),
                            Err(e) => {
                                if e.to_string().contains("Timeout") {
                                    p.mark_timeout(&xid);
                                } else {
                                    p.mark_error(&xid, &e.to_string());
                                }
                            }
                        }
                    } else if let Some(ref s) = streaming {
                        match &result {
                            Ok(_) => s.success(&name, None),
                            Err(e) => {
                                if e.to_string().contains("Timeout") {
                                    s.timeout(&name);
                                } else {
                                    s.error(&name, &e.to_string());
                                }
                            }
                        }
                    }

                    results.lock().await.push((xid, name, result));
                });
                handles.push(handle);
            }

            // Wait for all tasks
            for handle in handles {
                let _ = handle.await;
            }
        })
        .await;

    // Finish progress display
    if let Some(ref p) = progress {
        p.finish();
    }

    // Build collection result
    let results = Arc::try_unwrap(results)
        .map_err(|_| anyhow::anyhow!("Failed to unwrap results"))?
        .into_inner();

    let mut successes = Vec::new();
    let mut rejections = Vec::new();
    let mut errors = Vec::new();
    let mut timeouts = Vec::new();

    for (xid, name, result) in results {
        match result {
            Ok(data) => successes.push((xid, data)),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("Timeout") {
                    timeouts.push(xid);
                } else if msg.contains("rejected") || msg.contains("Rejected") {
                    rejections.push((xid, format!("{}: {}", name, msg)));
                } else {
                    errors.push((xid, format!("{}: {}", name, msg)));
                }
            }
        }
    }

    Ok(CollectionResult { successes, rejections, errors, timeouts })
}

/// Send messages to multiple participants in parallel.
///
/// Uses `tokio::task::LocalSet` because Hubert's `KvStore` futures are `!Send`.
pub async fn parallel_send(
    client: Arc<StorageClient>,
    messages: Vec<(XID, ARID, Envelope, String)>,
) -> Vec<(XID, Result<()>)> {
    let is_interactive = is_interactive_terminal();
    let message_count = messages.len();

    // Set up progress display for put operations
    let progress = if is_interactive {
        let p = Arc::new(ProgressDisplay::new_put(
            &messages
                .iter()
                .map(|(xid, _, _, name)| (*xid, name.clone()))
                .collect::<Vec<_>>(),
        ));
        p.start_timer_updates();
        Some(p)
    } else {
        None
    };

    let streaming = if !is_interactive {
        let s = StreamingOutput::new_put(true);
        s.started(message_count);
        Some(Arc::new(s))
    } else {
        None
    };

    // Track start time for elapsed calculation in streaming mode
    let start_time = std::time::Instant::now();

    #[allow(clippy::type_complexity)]
    let results: Arc<Mutex<Vec<(XID, Result<()>)>>> =
        Arc::new(Mutex::new(Vec::new()));

    let local_set = tokio::task::LocalSet::new();

    local_set
        .run_until(async {
            let mut handles = Vec::new();

            for (xid, arid, envelope, name) in messages {
                let client = Arc::clone(&client);
                let results = Arc::clone(&results);
                let progress = progress.clone();
                let streaming = streaming.clone();
                let start = start_time;

                let handle = tokio::task::spawn_local(async move {
                    let result = client.put(&arid, &envelope).await.map(|_| ());
                    let elapsed = start.elapsed().as_secs();

                    if let Some(ref p) = progress {
                        match &result {
                            Ok(()) => p.mark_success(&xid),
                            Err(e) => p.mark_error(&xid, &e.to_string()),
                        }
                    } else if let Some(ref s) = streaming {
                        match &result {
                            Ok(()) => s.success(&name, Some(elapsed)),
                            Err(e) => s.error(&name, &e.to_string()),
                        }
                    }

                    results.lock().await.push((xid, result));
                });
                handles.push(handle);
            }

            for handle in handles {
                let _ = handle.await;
            }
        })
        .await;

    if let Some(ref p) = progress {
        p.finish();
    }

    Arc::try_unwrap(results)
        .expect("all tasks completed")
        .into_inner()
}

/// Helper to build request tuples from pending requests and registry.
pub fn build_fetch_requests<'a>(
    pending: impl Iterator<Item = (&'a XID, &'a ARID)>,
    get_name: impl Fn(&XID) -> String,
) -> Vec<(XID, ARID, String)> {
    pending
        .map(|(xid, arid)| {
            let name = get_name(xid);
            (*xid, *arid, name)
        })
        .collect()
}
