# Priority Tasks for `frost` Crate

## Current State Summary

The `frost` CLI is a working tool for managing FROST (Flexible Round-Optimized Schnorr Threshold) participants. Current functionality:

1. **Registry Management** (`frost registry`)
   - Owner set with private XID documents
   - Participant add with signed public XID documents
   - Persistent JSON storage

2. **DKG Invite Flow** (`frost dkg invite`)
   - `send`: Coordinator creates sealed/unsealed invites for participants
   - `receive`: Participants fetch and decrypt invites from Hubert or local envelope
   - `respond`: Participants accept or reject, posting response to Hubert

3. **DKG Round 1** (`frost dkg round1`)
   - `collect`: Coordinator fetches all participant responses from Hubert, validates GSTP responses, extracts Round 1 packages, saves to `collected_round1.json`

4. **DKG Round 2** (`frost dkg round2`)
   - `send`: Coordinator sends individual sealed messages to each participant containing all Round 1 packages and their unique response ARID (posts to ARIDs participants specified in their invite responses)
   - `respond`: Participants now respond (implemented) with round2 packages, persist round2 secret, include next `response_arid`, and update `listening_at_arid`

5. **Storage Backends**
   - Hubert server (HTTP)
   - Mainline DHT
   - IPFS
   - Hybrid (DHT + IPFS)

6. **Demo Script** (`frost-demo.py`)
   - Provisions 4 participants (Alice, Bob, Carol, Dan) in separate directories
   - Builds registries
   - Creates and responds to DKG invites via Hubert
   - Coordinator collects Round 1 packages
   - Coordinator sends Round 2 request

## Where the Demo Stops

The `demo-log.md` now runs through participants responding to Round 2 (including unsealed preview) and posting to Hubert. Each participant has:
- `registry.json` - Group membership, pending_requests (Round 2), updated `listening_at_arid` for finalize
- `group-state/<group-id>/round1_secret.json` - Round 1 secret (participants only)
- `group-state/<group-id>/round1_package.json` - Round 1 package (participants only)
- `group-state/<group-id>/collected_round1.json` - All Round 1 packages (coordinator only)
- `group-state/<group-id>/round2_secret.json` - Round 2 secret (participants only)

## Next Steps (Priority Order)

### 1. Coordinator Collects Round 2

**Command (to implement):** `frost dkg round2 collect`

The coordinator:
- Fetches all Round 2 responses from Hubert (using `collect_from_arid` from pending_requests)
- Validates each response (function, group, recipient, etc.)
- Saves collected Round 2 packages to `collected_round2.json`
- Updates pending_requests for finalize fan-out (send_to/collect_from)

### 2. Coordinator Distributes Round 2 Packages

**Command:** `frost dkg finalize send`

The coordinator:
- Redistributes each participant's incoming Round 2 packages to them
- Each participant receives only the packages destined for them

### 3. Participants Finalize Key Generation

**Command:** `frost dkg finalize respond`

Each participant:
- Fetches their incoming Round 2 packages
- Runs `frost_ed25519::keys::dkg::part3` with their Round 2 secret and incoming packages
- Produces `KeyPackage` and `PublicKeyPackage`
- Stores `key_package.json`
- Posts confirmation to coordinator

### 5. Group Status and Listing

**Commands:**
- `frost group list` - List all groups in registry with status
- `frost group info <GROUP_ID>` - Show group details, participants, coordinator, signing threshold

### 6. Threshold Signing

Once key generation is complete:
- `frost sign start` - Coordinator initiates signing session
- `frost sign commit` - Participant sends commitment
- `frost sign contribute` - Participant sends signature share
- `frost sign finish` - Coordinator aggregates shares into final signature

## Implementation Notes

### GroupRecord Enhancements

The `ContributionPaths` structure now used for Round 2:
```rust
pub struct ContributionPaths {
    pub round1_secret: Option<String>,
    pub round1_package: Option<String>,
    pub round2_secret: Option<String>,  // Populated during round2 respond
    pub key_package: Option<String>,    // Ready for finalize
}
```

The `PendingRequests` structure tracks response ARIDs across phases:
```rust
pub struct PendingRequests {
    requests: Vec<PendingRequest>,  // Maps participant XID to send_to / collect_from
}
```

### GSTP Flow

The pattern is established:
1. Coordinator sends `SealedRequest` with function name and parameters
2. Participants decrypt, validate, process
3. Participants send `SealedResponse` with result or error
4. Coordinator collects responses

### Test Coverage

Add integration tests for:
- Round 2 message construction and distribution
- Key package generation
- End-to-end signing flow

### Demo Script Updates

- Demo now includes unsealed Round 2 response preview (no state change) followed by sealed post with `--verbose` for Hubert interactions.
- After implementing subsequent phases, extend `frost-demo.py` to cover round2 collect, finalize send/respond, and signing.

## Lower Priority Enhancements

- **Timeout handling** for unresponsive participants
- **Resharing** when participants need to be replaced
- **Key rotation** for long-lived groups
- **Audit logging** of all DKG and signing operations
- **Recovery mode** if a participant loses state mid-protocol
