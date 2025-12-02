# Coordinator Parallelization Implementation Plan

## Implementation Status

| Phase    | Description                               | Status     |
| -------- | ----------------------------------------- | ---------- |
| Phase 1  | Add Dependencies                          | ✅ Complete |
| Phase 2  | Create Parallel Fetch Module              | ✅ Complete |
| Phase 3  | Interactive Progress Display              | ✅ Complete |
| Phase 4  | Non-Interactive Output                    | ✅ Complete |
| Phase 5  | Parallel Execution Core                   | ✅ Complete |
| Phase 6  | Refactor dkg coordinator round1           | ✅ Complete |
| Phase 7  | Parallel Sends                            | ✅ Complete |
| Phase 8  | Terminal Detection                        | ✅ Complete |
| Phase 9  | Add --parallel flag to remaining commands | ✅ Complete |
| Phase 10 | Error Handling Strategy                   | ✅ Complete |

## All Commands Updated

All coordinator commands now have `--parallel` flag:

- `dkg coordinator round1 --parallel`
- `dkg coordinator round2 --parallel`
- `dkg coordinator finalize --parallel`
- `sign coordinator round1 --parallel`
- `sign coordinator round2 --parallel`

## Lessons Learned

### Critical: Hubert KvStore futures are `!Send`

The Hubert `KvStore` trait uses `#[async_trait(?Send)]`, meaning its futures **cannot** be spawned across threads with `tokio::spawn`.

**What doesn't work:**
```rust
// ERROR: future is not Send
tokio::spawn(async move {
    client.get(&arid, None).await  // !Send future
});
```

**What works:**
Using `futures::future::join_all` with regular async blocks (not spawned tasks) allows concurrent execution without requiring `Send`:

```rust
use futures::future::join_all;

let futures: Vec<_> = arids
    .iter()
    .map(|arid| async {
        client.get(arid, timeout).await
    })
    .collect();

let results = join_all(futures).await;
```

This works because `join_all` runs all futures on the same thread, polling them interleaved, without spawning separate tasks.

### Arc unwrap requires avoiding `expect`

When using `Arc::try_unwrap`, the error type contains `Arc<T>`, which requires `T: Debug` for `.expect()`. Use `.map_err()` instead:

```rust
// ERROR if T doesn't impl Debug
Arc::try_unwrap(results).expect("all tasks completed")

// Works for any T
Arc::try_unwrap(results)
    .map_err(|_| anyhow::anyhow!("Failed to unwrap"))?
```

### Progress display with indicatif

The `indicatif` crate provides multi-line progress display. Key patterns:

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

let multi = MultiProgress::new();
let bar = multi.add(ProgressBar::new_spinner());
bar.set_message("Bob");
bar.enable_steady_tick(Duration::from_millis(100));

// Update status
bar.set_style(ProgressStyle::default_spinner()
    .template("✅ {msg}").unwrap());
bar.finish();
```

### Parallel fetch/send architecture

The implementation uses a channel-based pattern:
1. Spawn all fetch/send operations as concurrent futures
2. Use `futures::future::join_all` to await all
3. Collect results with status tracking
4. Update progress display as each completes

```rust
pub struct CollectionResult<T> {
    pub successes: Vec<(XID, T)>,
    pub rejections: Vec<(XID, String)>,
    pub errors: Vec<(XID, String)>,
    pub timeouts: Vec<XID>,
}
```

## Overview

This document describes an implementation plan to parallelize the coordinator's send and receive operations for the following commands:

- `dkg coordinator round1`
- `dkg coordinator round2`
- `dkg coordinator finalize`
- `sign coordinator round1`
- `sign coordinator round2`

Currently, these commands fetch responses sequentially from each participant (Bob, Carol, Dan, etc.) and then send messages sequentially. Given that Hubert operations (mainline DHT, IPFS, hybrid) can take significant time, and participants may respond at different times and in any order, parallel execution can substantially reduce total wait time.

## Current Architecture

### Sequential Pattern

Each coordinator command follows a similar pattern:

```rust
for (participant_xid, collect_from_arid) in pending_requests.iter_collect() {
    // 1. Print participant name
    if is_verbose() { eprintln!("{}...", participant_name); }

    // 2. Fetch response from Hubert (blocking)
    let envelope = runtime.block_on(async {
        client.get(response_arid, timeout).await
    })?;

    // 3. Validate and extract data
    // 4. Collect into results vec
}

for (xid, recipient_doc, send_to_arid, collect_from_arid) in &participant_info {
    // 1. Build request
    // 2. Send to Hubert (blocking)
    runtime.block_on(async {
        client.put(send_to_arid, &sealed_envelope).await
    })?;
}
```

### Problems with Sequential Approach

1. **Latency accumulation**: If each Hubert get takes 5-30 seconds (DHT/IPFS), waiting for 3 participants = 15-90 seconds total.
2. **No visibility**: Users see "Bob..." then nothing for 30 seconds.
3. **No early failure detection**: A rejection from Carol doesn't surface until after Bob completes.
4. **Suboptimal user experience**: No indication of progress or which participants are slow.

## Target Architecture

### Parallel Fetch with Progress Display

```rust
// Spawn all fetches concurrently
let handles: Vec<_> = pending_requests.iter_collect()
    .map(|(xid, arid)| {
        let client = client.clone();
        tokio::spawn(async move {
            client.get(arid, timeout).await
        })
    })
    .collect();

// Poll for completion with progress updates
while !all_complete {
    // Update terminal display
    // Check for new completions
}
```

### Interactive Terminal Display

For interactive terminals (tty), display a live-updating status:

```
⏳ Bob
✅ Carol
⏳ Dan
...waiting for 35 more seconds...
```

### Non-Interactive Terminal Display

For non-interactive terminals (pipes, CI), print status updates as they occur:

```
Waiting for responses...
✅ Carol
⏳ Bob
❌ Dan - Rejected: "Busy right now"
```

## Implementation Plan

### Phase 1: Add Dependencies

Add terminal UI dependency to `Cargo.toml`:

```toml
[dependencies]
# For interactive terminal progress display
indicatif = "0.17"  # Progress bars and spinners

# Already present:
# tokio = { version = "1", features = ["rt-multi-thread", "time"] }
```

Alternative options:
- `console` crate: Lower-level terminal manipulation
- `crossterm`: Cross-platform terminal control
- `ratatui`: Full TUI framework (overkill for this use case)

`indicatif` is recommended because:
- Simple API for multi-line progress display
- Handles terminal detection (tty vs pipe)
- Well-maintained and widely used
- Supports spinner + status message pattern

### Phase 2: Create Parallel Fetch Module

Create `src/cmd/parallel.rs`:

```rust
use std::collections::HashMap;
use std::time::Duration;
use anyhow::{Result, Context};
use bc_components::{ARID, XID};
use bc_envelope::Envelope;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::mpsc;
use tokio::time::timeout as tokio_timeout;

use crate::cmd::storage::StorageClient;

/// Status of a participant's response fetch
#[derive(Debug, Clone)]
pub enum FetchStatus {
    Pending,
    Success(Envelope),
    Rejected(String),
    Error(String),
    Timeout,
}

/// Result of a parallel fetch operation
pub struct ParallelFetchResult {
    pub results: HashMap<XID, FetchStatus>,
    pub successful: Vec<(XID, Envelope)>,
    pub failed: Vec<(XID, String)>,
}

/// Configuration for parallel fetch
pub struct ParallelFetchConfig {
    pub timeout_seconds: Option<u64>,
    pub poll_interval: Duration,
}

impl Default for ParallelFetchConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: Some(600), // 10 minutes default
            poll_interval: Duration::from_millis(500),
        }
    }
}

/// Fetch responses from multiple participants in parallel with progress display.
pub async fn parallel_fetch<F, T>(
    client: &StorageClient,
    requests: Vec<(XID, ARID, String)>,  // (participant_xid, arid, display_name)
    config: ParallelFetchConfig,
    validate_and_extract: F,
) -> Result<ParallelFetchResult>
where
    F: Fn(&Envelope, &XID) -> Result<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    // Implementation details below
    todo!()
}
```

### Phase 3: Interactive Progress Display

```rust
/// Display progress for interactive terminals
struct InteractiveProgress {
    multi: MultiProgress,
    bars: HashMap<XID, ProgressBar>,
}

impl InteractiveProgress {
    fn new(participants: &[(XID, String)]) -> Self {
        let multi = MultiProgress::new();
        let style_pending = ProgressStyle::default_spinner()
            .template("{spinner:.yellow} {msg}")
            .unwrap();
        let style_done = ProgressStyle::default_spinner()
            .template("✅ {msg}")
            .unwrap();
        let style_error = ProgressStyle::default_spinner()
            .template("❌ {msg}")
            .unwrap();

        let mut bars = HashMap::new();
        for (xid, name) in participants {
            let bar = multi.add(ProgressBar::new_spinner());
            bar.set_style(style_pending.clone());
            bar.set_message(name.clone());
            bar.enable_steady_tick(Duration::from_millis(100));
            bars.insert(*xid, bar);
        }

        Self { multi, bars }
    }

    fn mark_success(&self, xid: &XID) {
        if let Some(bar) = self.bars.get(xid) {
            bar.set_style(ProgressStyle::default_spinner()
                .template("✅ {msg}")
                .unwrap());
            bar.finish();
        }
    }

    fn mark_error(&self, xid: &XID, error: &str) {
        if let Some(bar) = self.bars.get(xid) {
            let msg = format!("{} - {}", bar.message(), error);
            bar.set_style(ProgressStyle::default_spinner()
                .template("❌ {msg}")
                .unwrap());
            bar.set_message(msg);
            bar.finish();
        }
    }

    fn update_timeout(&self, remaining_seconds: u64) {
        // Update footer with remaining time
    }
}
```

### Phase 4: Non-Interactive Output

```rust
/// Output for non-interactive terminals (pipes, CI)
struct StreamingOutput {
    verbose: bool,
}

impl StreamingOutput {
    fn started(&self) {
        if self.verbose {
            eprintln!("Waiting for responses...");
        }
    }

    fn success(&self, name: &str) {
        eprintln!("✅ {}", name);
    }

    fn error(&self, name: &str, error: &str) {
        eprintln!("❌ {} - {}", name, error);
    }
}
```

### Phase 5: Parallel Execution Core

```rust
pub async fn parallel_fetch_impl<F, T>(
    client: &StorageClient,
    requests: Vec<(XID, ARID, String)>,
    config: ParallelFetchConfig,
    validate: F,
) -> Result<HashMap<XID, Result<T>>>
where
    F: Fn(&Envelope, &XID) -> Result<T> + Clone + Send + Sync + 'static,
    T: Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<(XID, Result<Envelope>)>(requests.len());

    // Spawn all fetch tasks
    let mut handles = Vec::new();
    for (xid, arid, _name) in &requests {
        let client = client.clone();
        let arid = *arid;
        let xid = *xid;
        let tx = tx.clone();
        let timeout_secs = config.timeout_seconds;

        let handle = tokio::spawn(async move {
            let result = match timeout_secs {
                Some(secs) => {
                    match tokio_timeout(
                        Duration::from_secs(secs),
                        client.get(&arid, None)
                    ).await {
                        Ok(Ok(Some(env))) => Ok(env),
                        Ok(Ok(None)) => Err(anyhow::anyhow!("Not found")),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(anyhow::anyhow!("Timeout")),
                    }
                }
                None => {
                    client.get(&arid, None).await
                        .and_then(|opt| opt.context("Not found"))
                }
            };
            let _ = tx.send((xid, result)).await;
        });
        handles.push(handle);
    }
    drop(tx);  // Close sender so receiver can complete

    // Collect results
    let mut results = HashMap::new();
    while let Some((xid, result)) = rx.recv().await {
        let validated = result.and_then(|env| validate(&env, &xid));
        results.insert(xid, validated);
    }

    // Wait for all tasks to complete
    for handle in handles {
        let _ = handle.await;
    }

    Ok(results)
}
```

### Phase 6: Refactor Coordinator Commands

Each coordinator command needs to be refactored to use the parallel fetch. Example for `dkg coordinator round1`:

#### Before (sequential):

```rust
fn fetch_all_round1_packages(
    ctx: &Round1Context<'_>,
    pending_requests: &PendingRequests,
    timeout: Option<u64>,
) -> Result<(Vec<Round1Package>, Vec<NextResponseArid>)> {
    for (participant_xid, collect_from_arid) in pending_requests.iter_collect() {
        // Sequential fetch...
    }
}
```

#### After (parallel):

```rust
async fn fetch_all_round1_packages_parallel(
    client: &StorageClient,
    registry: &Registry,
    pending_requests: &PendingRequests,
    coordinator: &XIDDocument,
    expected_group_id: &ARID,
    config: ParallelFetchConfig,
) -> Result<(Vec<Round1Package>, Vec<NextResponseArid>)> {
    let requests: Vec<_> = pending_requests.iter_collect()
        .map(|(xid, arid)| {
            let name = registry.participant(xid)
                .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                .unwrap_or_else(|| xid.ur_string());
            (*xid, *arid, name)
        })
        .collect();

    let coordinator_keys = coordinator.inception_private_keys()
        .context("Missing private keys")?;
    let group_id = *expected_group_id;

    let results = parallel_fetch_with_progress(
        client,
        requests,
        config,
        move |envelope, xid| {
            validate_and_extract_round1(envelope, &coordinator_keys, &group_id)
        },
    ).await?;

    // Collect successful results, report failures
    // ...
}
```

### Phase 7: Parallel Sends

Sending messages can also be parallelized:

```rust
async fn parallel_send(
    client: &StorageClient,
    messages: Vec<(XID, ARID, Envelope)>,  // (recipient, arid, sealed_envelope)
) -> Result<Vec<(XID, Result<()>)>> {
    let handles: Vec<_> = messages.into_iter()
        .map(|(xid, arid, envelope)| {
            let client = client.clone();
            tokio::spawn(async move {
                let result = client.put(&arid, &envelope).await;
                (xid, result.map(|_| ()))
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }

    Ok(results)
}
```

### Phase 8: Terminal Detection

```rust
fn is_interactive_terminal() -> bool {
    std::io::stderr().is_terminal()
}

/// Choose display strategy based on terminal type
enum ProgressDisplay {
    Interactive(InteractiveProgress),
    Streaming(StreamingOutput),
}

impl ProgressDisplay {
    fn new(participants: &[(XID, String)], verbose: bool) -> Self {
        if is_interactive_terminal() {
            Self::Interactive(InteractiveProgress::new(participants))
        } else {
            Self::Streaming(StreamingOutput { verbose })
        }
    }
}
```

### Phase 9: Commands to Modify

Each command gains a `--parallel` flag. When omitted, the existing sequential behavior is preserved (for backward compatibility and debugging). When provided, parallel fetch/send with progress display is used.

| Command                    | Fetch                | Send                | `--parallel` behavior                        |
| -------------------------- | -------------------- | ------------------- | -------------------------------------------- |
| `dkg coordinator round1`   | Round1 responses     | Round2 requests     | Parallel fetch + parallel send with progress |
| `dkg coordinator round2`   | Round2 responses     | Finalize requests   | Parallel fetch + parallel send with progress |
| `dkg coordinator finalize` | Finalize responses   | (none)              | Parallel fetch with progress                 |
| `sign coordinator round1`  | signInvite responses | signRound2 requests | Parallel fetch + parallel send with progress |
| `sign coordinator round2`  | Signature shares     | Finalize events     | Parallel fetch + parallel send with progress |

#### CLI Flag Definition

Add to each coordinator command's `CommandArgs`:

```rust
/// Use parallel fetch/send with interactive progress display
#[arg(long)]
parallel: bool,
```

#### Conditional Execution

```rust
impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        // ... setup code ...

        if self.parallel {
            // New parallel path with progress display
            let collection = runtime.block_on(async {
                collect_round1_responses_parallel(&client, ...).await
            })?;
        } else {
            // Existing sequential path (unchanged)
            let collection = collect_round1_responses(&mut ctx, ...)?;
        }

        // ... rest of command ...
    }
}
```

This approach:
- Preserves existing behavior by default (no breaking changes)
- Allows incremental testing of parallel mode
- Makes parallel mode opt-in until it's proven stable
- Provides an escape hatch if parallel mode has issues

### Phase 10: Error Handling Strategy

With parallel execution, we need to decide how to handle partial failures.

**Decision: Collect all results, then report**

- Wait for all responses (or timeouts)
- Report complete status at the end
- Fail if threshold not met

This approach provides better UX: the coordinator may be able to proceed with a quorum (e.g., 2-of-3 in signing), and users benefit from seeing the full picture before any error is raised.

```rust
struct CollectionResult<T> {
    successes: Vec<(XID, T)>,
    rejections: Vec<(XID, String)>,  // Participant explicitly rejected
    errors: Vec<(XID, String)>,       // Network/parsing errors
    timeouts: Vec<XID>,               // No response within timeout
}

impl<T> CollectionResult<T> {
    fn can_proceed(&self, min_required: usize) -> bool {
        self.successes.len() >= min_required
    }
}
```

## Testing Strategy

### Unit Tests

1. Test `parallel_fetch` with mock `StorageClient`
2. Test progress display output formatting
3. Test error aggregation logic

### Integration Tests

1. Run against local Hubert server with simulated delays
2. Test timeout behavior
3. Test partial failure scenarios

### Demo Script Updates

Update `frost-demo.py` to verify parallel behavior:
- Add timing measurements
- Verify correct output ordering
- Test with intentional delays

## Migration Path

1. **Phase 1**: Add `indicatif` dependency, create `parallel.rs` module
2. **Phase 2**: Implement parallel fetch with progress display
3. **Phase 3**: Refactor `dkg coordinator round1` as first target
4. **Phase 4**: Verify with demo script, adjust UX
5. **Phase 5**: Refactor remaining 4 coordinator commands
6. **Phase 6**: Update demo script output expectations
7. **Phase 7**: Add parallel sends

## Open Questions — Resolved

Based on analysis of the Hubert crate's API, CLI, tests, and demos:

### 1. Polling interval: How often to check for new completions?

**Answer: 1000ms (1 second)**

Evidence from Hubert source code:
- `mainline/kv.rs` line 142: `let poll_interval = Duration::from_millis(1000);` — "Changed to 1000ms for verbose mode polling"
- `server/kv.rs` line 105: `let poll_interval = Duration::from_millis(1000);` — Same pattern
- `tests/common/kv_tests.rs` line 17: `const RETRY_DELAY_MS: u64 = 500;` — Test code uses 500ms for faster testing

**Recommendation**: Use **1000ms** to match Hubert's production behavior. The 500ms in tests is for faster test execution, not production use. For parallel fetch, since we're spawning concurrent tasks, we don't need to poll per-participant; instead, we await all futures and receive results as they complete via `mpsc::channel`.

### 2. Timeout display: Show countdown for timeout?

**Answer: Yes, continuous countdown for interactive terminals**

Evidence from Hubert:
- CLI default timeout is 30 seconds (`--timeout` defaults to 30)
- Server tests use `Some(30)` as standard timeout
- Test in `test_server.rs` line 217-231 validates timeout behavior, expecting ~2 seconds when `timeout=2`
- Verbose mode prints dots during polling (`verbose_print_dot()`)

**Recommendation**: For interactive terminals, display a continuous countdown updated every second. Using `indicatif`, this is essentially free — the progress bar/spinner ticks automatically. Show format like:

```
⏳ Bob
⏳ Carol
✅ Dan
Waiting... 25s remaining
```

The countdown updates every second via `indicatif`'s steady tick, which runs on a background thread and has negligible overhead. When a participant completes, update their line. The footer countdown keeps users informed without requiring any polling cost — we're already awaiting the futures.

### 3. Cancel handling: Allow Ctrl-C to abort gracefully?

**Answer: Yes, via tokio signal handling**

Evidence from Hubert:
- Hubert server uses `axum` with multi-threaded tokio runtime
- No explicit signal handling in Hubert code (relies on tokio defaults)
- The CLI is designed for short-lived operations

**Recommendation**: Add `tokio::signal::ctrl_c()` handler in the parallel fetch loop. When Ctrl-C is received:
1. Cancel all in-flight fetch tasks
2. Report partial results (which participants responded before cancellation)
3. Exit with non-zero status

Implementation sketch:
```rust
tokio::select! {
    _ = tokio::signal::ctrl_c() => {
        eprintln!("\nCancelled. Partial results:");
        report_current_state(&completed);
        return Err(anyhow::anyhow!("Cancelled by user"));
    }
    result = fetch_all => { ... }
}
```

### 4. Quorum early-exit: For signing, exit once min_signers respond?

**Answer: Optional flag (`--quorum-exit`), disabled by default**

Evidence from Hubert:
- Hubert's `KvStore::get()` has no concept of quorum — it's single-value retrieval
- The FROST signing protocol requires exactly `min_signers` signatures to produce a valid aggregate
- DKG requires all participants (no quorum)

**Recommendation**:
- **DKG commands**: Always wait for all participants (no early exit)
- **Sign commands**: Add `--quorum-exit` flag:
  - When enabled: Exit as soon as `min_signers` successful responses received
  - When disabled (default): Wait for all invited participants
  - Rationale: Users may want to see all responses for auditing, even if quorum is met

Example:
```bash
# Wait for all 3 participants (default)
frost sign coordinator round1 --storage server $SESSION_ID

# Exit once 2 responses received (min_signers=2)
frost sign coordinator round1 --storage server --quorum-exit $SESSION_ID
```

## Appendix: Symbol Reference

| Symbol | Meaning                             |
| ------ | ----------------------------------- |
| ⏳      | Waiting for response                |
| ✅      | Response received and validated     |
| ❌      | Response rejected or error occurred |
| 🔄      | (Alternative) Request in progress   |

## File Changes Summary

| File                                  | Action                                 |
| ------------------------------------- | -------------------------------------- |
| `Cargo.toml`                          | Add `indicatif = "0.17"`               |
| `src/cmd/mod.rs`                      | Add `pub mod parallel;`                |
| `src/cmd/parallel.rs`                 | **New**: Parallel fetch/send utilities |
| `src/cmd/dkg/coordinator/round1.rs`   | Refactor to use parallel fetch         |
| `src/cmd/dkg/coordinator/round2.rs`   | Refactor to use parallel fetch         |
| `src/cmd/dkg/coordinator/finalize.rs` | Refactor to use parallel fetch         |
| `src/cmd/sign/coordinator/round1.rs`  | Refactor to use parallel fetch         |
| `src/cmd/sign/coordinator/round2.rs`  | Refactor to use parallel fetch         |
| `frost-demo.py`                       | Update output expectations             |
