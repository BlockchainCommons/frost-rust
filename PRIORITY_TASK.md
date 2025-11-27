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

6. **Storage Backends**
   - Hubert server (HTTP)
   - Mainline DHT
   - IPFS
   - Hybrid (DHT + IPFS)

7. **Demo Script** (`frost-demo.py`)
   - Provisions 4 participants (Alice, Bob, Carol, Dan) in separate directories
   - Builds registries
   - Creates and responds to DKG invites via Hubert
   - Coordinator collects Round 1 packages
   - Coordinator sends Round 2 request
   - Coordinator collects Round 2 responses
   - Coordinator sends finalize requests
   - Participants post finalize responses
   - Coordinator collects finalize responses and outputs the group verifying key
   - Signing: builds a sample target envelope, previews `sign start` (signCommit) unsealed, and posts it to Hubert

## Where the Demo Stops (current)

The `demo-log.md` now runs through finalize collect. Each participant has:
- `registry.json` - Group membership, pending_requests (Round 2), updated `listening_at_arid` for finalize
- `group-state/<group-id>/round1_secret.json` - Round 1 secret (participants only)
- `group-state/<group-id>/round1_package.json` - Round 1 package (participants only)
- `group-state/<group-id>/collected_round1.json` - All Round 1 packages (coordinator only)
- `group-state/<group-id>/round2_secret.json` - Round 2 secret (participants only)
- `group-state/<group-id>/collected_round2.json` - Round 2 packages keyed by sender/recipient plus next `response_arid`
- Finalize requests sent and participants respond with key/public key packages
- `group-state/<group-id>/collected_finalize.json` - Finalize responses; coordinator prints group verifying key (`ur:signing-public-key/...`)
- Registry now also records `verifying_key` (UR) for both coordinator and participants after finalize respond/collect

## Next Steps (Priority Order)

### 1. Threshold Signing Flow (status + implementation order)

- Implemented: `frost sign start` (coordinator) with first-hop ARID, per-participant commit/share ARIDs, full target envelope, preview and Hubert post.
- Pending: `frost sign commit`, `frost sign collect`, `frost sign share`, `frost sign finish`.

1) **`frost sign start` (coordinator)**
   - Inputs: group ID; target envelope (assumed already wrapped as needed).
   - Derive: session ID (ARID) and target digest = digest(subject(target envelope)).
   - Generate:
     - a single first-hop ARID (write-once) where *each* participant retrieves the initial request (print its UR on output, same pattern as `dkg invite send`),
     - per-participant commitment ARIDs (each participant gets a unique ARID to post their commitment; coordinator polls each) to be carried as `response_arid` fields inside the initial request (Hubert pattern: each message tells the peer where to respond),
     - per-participant share ARIDs (where participants post signature shares) to be delivered as the next-hop `response_arid` inside the “signShare” request (again, the message carries the next ARID, not pre-agreed out-of-band).
   - Build initial GSTP “signCommit” request with parameters: group, targetDigest, minSigners/participant list, commitmentCollectArid, per-participant shareArid (individually encrypted to each participant inside the body).
   - Multicast pattern = DKG invite: per-participant response ARIDs encrypted under inner `recipient` assertion, then the whole GSTP request encrypted to all participants (multiple GSTP `recipient` assertions). No participant can see others’ ARIDs.
   - Post to each participant’s `send_to_arid` (from registry pending_requests). Preview mode (`--preview`) prints unsealed request for one participant; sealed mode posts with `--verbose` as desired.
   - Persist session state under `group-state/<group-id>/signing/<session-id>/start.json` (participant list, target digest, ARIDs).

2) **`frost sign commit` (participant)**
   - Fetch “signCommit” request from current `listening_at_arid`.
   - Validate function/group/participant list; extract per-participant shareArid and commitmentCollectArid for self.
   - Run FROST signing part1 to produce commitment(s); generate next `response_arid` for share response.
   - Post GSTP response with commitments and `response_arid` to coordinator’s commitmentCollectArid (Hubert). Preview mode prints unsealed response only. Update local `listening_at_arid` to shareArid; persist part1 output under `group-state/<group-id>/signing/<session-id>/commit.json`.

3) **`frost sign collect` (coordinator)**
   - Collect all “signCommit” responses from commitmentCollectArid.
   - Validate group/session IDs and participants; aggregate commitments.
   - Build per-participant “signShare” GSTP request carrying aggregated commitments and each participant’s shareArid (where they will post their signature share). Pattern: 1-1 sealed delivery (no inner per-recipient ARIDs needed).
   - Update registry pending_requests for the signing session, and persist commitments under `group-state/<group-id>/signing/<session-id>/commitments.json`.

4) **`frost sign share` (participant)**
   - Fetch “signShare” request from `listening_at_arid`.
   - Validate group/session IDs; run FROST signing part2 to produce signature share using stored part1 state and aggregated commitments.
   - Generate next `response_arid` (if further interaction needed; otherwise omit) and post GSTP response with signature share to coordinator’s share-collection ARID. Persist share under `group-state/<group-id>/signing/<session-id>/share.json`. Clear `listening_at_arid` when done.

5) **`frost sign finish` (coordinator)**
   - Collect all signature shares from the share-collection ARID; validate.
   - Aggregate to final `Signature::Ed25519`.
   - Persist final signature and session transcript under `group-state/<group-id>/signing/<session-id>/final.json`; print UR form for the signature. Any participant can attach it as `'signed': Signature` to the target envelope (CBOR `Signature(...)` in envelopes).

**General patterns reused from DKG:**
- ARID flow naming: `send_to_arid` (where coordinator posts), `collect_from_arid` (where coordinator polls), participant `response_arid`, local `listening_at_arid`.
- Previews (`--preview`) are non-mutating and skip Hubert posts; `--verbose` shows Hubert transfers.
- All signing is over the digest of the *subject* of the target envelope.
- Store session artifacts under `group-state/<group-id>/signing/<session-id>/...`.

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

Group records now persist the aggregated `verifying_key` (UR `ur:signing-public-key/...`) once finalize respond/collect completes; merging enforces consistency across updates.

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
