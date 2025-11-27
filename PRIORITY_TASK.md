# Priority Tasks for `frost` Crate

## Current State Summary

The `frost` CLI is a working tool for managing FROST (Flexible Round-Optimized Schnorr Threshold) participants. Current functionality:

1. **Registry Management** (`frost registry`)
   - Owner set with private XID documents
   - Participant add with signed public XID documents
   - Persistent JSON storage

2. **DKG Invite Flow** (`frost dkg invite`)
   - `send`: Coordinator creates sealed/preview invites for participants
   - `receive`: Participants fetch and decrypt invites from Hubert or local envelope
   - `respond`: Participants accept or reject, posting response to Hubert

3. **DKG Round 1** (`frost dkg round1`)
   - `collect`: Coordinator fetches all participant responses from Hubert, validates GSTP responses, extracts Round 1 packages, saves to `collected_round1.json`

4. **DKG Round 2** (`frost dkg round2`)
   - `send`: Coordinator sends individual sealed messages to each participant containing all Round 1 packages and their unique response ARID (posts to ARIDs participants specified in their invite responses)
   - `respond`: Participants respond with round2 packages, persist round2 secret, include next `response_arid`, and update `listening_at_arid`
   - `collect`: Coordinator fetches/validates Round 2 responses, saves `collected_round2.json`, and updates pending_requests for finalize phase

5. **DKG Finalize** (`frost dkg finalize`)
   - `send`: Coordinator distributes collected Round 2 packages to each participant (with new `responseArid` for finalize respond)
   - `respond`: Participants run part3, produce key/public key packages, persist them, and return finalize response
   - `collect`: Coordinator collects finalize responses, writes `collected_finalize.json`, clears pending requests, and reports the group verifying key (`SigningPublicKey::Ed25519`, UR form `ur:signing-public-key`)

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
   - Coordinator collects Round 2 responses
   - Coordinator sends finalize requests
   - Participants post finalize responses
   - Coordinator collects finalize responses and outputs the group verifying key

## Where the Demo Stops

The `demo-log.md` now runs through finalize collect. Each participant has:
- `registry.json` - Group membership, pending_requests (Round 2), updated `listening_at_arid` for finalize
- `group-state/<group-id>/round1_secret.json` - Round 1 secret (participants only)
- `group-state/<group-id>/round1_package.json` - Round 1 package (participants only)
- `group-state/<group-id>/collected_round1.json` - All Round 1 packages (coordinator only)
- `group-state/<group-id>/round2_secret.json` - Round 2 secret (participants only)
- `group-state/<group-id>/collected_round2.json` - Round 2 packages keyed by sender/recipient plus next `response_arid`
- Finalize requests sent and participants respond with key/public key packages
- `group-state/<group-id>/collected_finalize.json` - Finalize responses; coordinator prints group verifying key (`ur:signing-public-key/...`)

## Next Steps (Priority Order)

### 1. Threshold Signing Flow

- Implement `frost sign start/commit/collect/share/finish` following the planned multicast first hop (per-participant response ARIDs embedded/encrypted in the initial request) and 1-1 messages thereafter.
- Persist signing session state under `group-state/<group-id>/signing/<session-id>/...` (commitments, shares, final signature).
- Ensure the final aggregated signature is `Signature::Ed25519` and can be attached as `'signed': Signature` to the target envelope.

### 2. Group Status and Listing

**Commands:**
- `frost group list` - List all groups in registry with status
- `frost group info <GROUP_ID>` - Show group details, participants, coordinator, signing threshold

### 3. Follow-on polish

- Thread the group verifying key into any status outputs where helpful (UR form in text, CBOR `SigningPublicKey(...)` in envelopes).
- Add integration tests for finalize collect and the upcoming signing flows.

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
- Finalize collect and group verifying key handling
- End-to-end signing flow

### Demo Script Updates

- Demo now includes Round 2 collect, finalize send/respond/collect, and prints the group verifying key.
- Next: extend `frost-demo.py` to cover the forthcoming signing flow once implemented.
