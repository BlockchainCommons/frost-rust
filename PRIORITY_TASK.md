# Priority Tasks for `frost` Crate

## Current State Summary

The `frost` CLI is a working tool for managing FROST (Flexible Round-Optimized Schnorr Threshold) participants. Current functionality:

1. **Registry Management** (`frost registry`)
   - Owner set with private XID documents
   - Participant add with signed public XID documents
   - Persistent JSON storage

2. **DKG Invite Flow** (`frost dkg coordinator invite` / `frost dkg participant invite`)
   - `send`: Coordinator creates sealed/preview invites for participants
   - `receive`: Participants fetch and decrypt invites from Hubert or local envelope
   - `respond`: Participants accept or reject, posting response to Hubert

3. **DKG Round 1** (`frost dkg coordinator round1`)
   - Coordinator fetches all participant responses from Hubert, validates GSTP responses, extracts Round 1 packages, saves to `collected_round1.json`, and posts individualized Round 2 requests (with optional preview)

4. **DKG Round 2** (`frost dkg coordinator round2 collect` / `frost dkg participant round2`)
   - `respond`: Participants respond with round2 packages, persist round2 secret, include next `response_arid`, and update `listening_at_arid`
   - `collect`: Coordinator fetches/validates Round 2 responses, saves `collected_round2.json`, and updates pending_requests for finalize phase

5. **DKG Finalize** (`frost dkg coordinator finalize` / `frost dkg participant finalize`)
   - `send`: Coordinator distributes collected Round 2 packages to each participant (with new `responseArid` for finalize respond)
   - `respond`: Participants run part3, produce key/public key packages, persist them, and return finalize response
   - `collect`: Coordinator collects finalize responses, writes `collected_finalize.json`, clears pending requests, and reports the group verifying key (`SigningPublicKey::Ed25519`, UR form `ur:signing-public-key`)

6. **Signing (in progress)**
   - `sign start` (coordinator): builds session, per-participant ARIDs (commit/share), target envelope, and posts signCommit requests
   - `sign receive` (participant): decrypts/validates signCommit, shows participants/target, persists session state + commit response ARID
   - `sign commit` (participant): generates nonces/commitments, posts signCommitResponse with next-hop share ARID, persists part1 state, updates listening ARID
   - `sign collect` (coordinator): collects all commitments, stores `commitments.json`, and dispatches per-participant signShare requests
   - `sign share` (participant): fetches signShare, validates session/commitments, produces signature share, posts to share ARID, and persists `share.json`

6. **Storage Backends**
   - Hubert server (HTTP)
   - Mainline DHT
   - IPFS
   - Hybrid (DHT + IPFS)

7. **Demo Script** (`frost-demo.py`)
   - Provisions 4 participants (Alice, Bob, Carol, Dan) in separate directories
   - Builds registries
   - Creates and responds to DKG invites via Hubert
   - Coordinator collects Round 1 packages and dispatches Round 2 requests
   - Coordinator collects Round 2 responses
   - Coordinator sends finalize requests
   - Participants post finalize responses
   - Coordinator collects finalize responses and outputs the group verifying key
   - Signing: builds a sample target envelope, previews `sign start` (signCommit), posts it, participants run signReceive/signCommit, coordinator runs signCollect and posts signShare requests, participants preview/post signShare responses

## Where the Demo Stops (current)

The `demo-log.md` now runs through finalize collect and participant signShare responses (no sign finish yet). Each participant has:
- `registry.json` - Group membership, pending_requests (Round 2), updated `listening_at_arid` for finalize
- `group-state/<group-id>/round1_secret.json` - Round 1 secret (participants only)
- `group-state/<group-id>/round1_package.json` - Round 1 package (participants only)
- `group-state/<group-id>/collected_round1.json` - All Round 1 packages (coordinator only)
- `group-state/<group-id>/round2_secret.json` - Round 2 secret (participants only)
- `group-state/<group-id>/collected_round2.json` - Round 2 packages keyed by sender/recipient plus next `response_arid`
- Finalize requests sent and participants respond with key/public key packages
- `group-state/<group-id>/collected_finalize.json` - Finalize responses; coordinator prints group verifying key (`ur:signing-public-key/...`)
- Registry now also records `verifying_key` (UR) for both coordinator and participants after finalize respond/collect
- Signing state:
  - Coordinator: `signing/<session>/start.json`, `commitments.json`
  - Participants: `signing/<session>/sign_receive.json`, `commit.json`, `share.json`

## Next Steps (Priority Order)

### 1. Threshold Signing Flow (status + implementation order)

- Implemented:
   - `frost sign coordinator start` (coordinator) with first-hop ARID, per-participant commit/share ARIDs, full target envelope, preview and Hubert post. Coordinator is no longer auto-added to the participant list; only actual signers are targeted.
   - `frost sign participant receive` (participant viewer): fetches/decrypts signCommit, validates sender/group/session/minSigners, shows sorted participants (lexicographic XID), formatted target envelope, persists request details (`sign_receive.json`) including response ARID for follow-up commands; no ARID printed to user (write-once).
   - `frost sign participant commit` (participant respond): uses persisted `sign_receive.json` (no Hubert re-fetch), supports `--preview` dry-run, optional `--reject`, derives commitments + next-hop share ARID, posts to coordinator’s commit ARID, and persists part1 state. Response body omits redundant participant field. Coordinator doc is resolved from the registry for encryption.
   - `frost sign coordinator collect` (coordinator): aggregates commitments, persists `commitments.json`, dispatches sealed signShare requests (with `--preview-share` option).
   - `frost sign participant share` (participant): retrieves signShare from listening ARID, validates session/minSigners/commitments/target digest from local state, posts signature share, persists `share.json`, clears listening ARID.
   - Pending: `frost sign finalize`.

1) ✅ **`frost sign coordinator start` (coordinator)**
   - Inputs: group ID; target envelope (assumed already wrapped as needed).
   - Derive: session ID (ARID) and target digest = digest(subject(target envelope)).
   - Generate:
     - a single first-hop ARID (write-once) where *each* participant retrieves the initial request (print its UR on output, same pattern as `frost dkg coordinator invite send`),
     - per-participant commitment ARIDs (each participant gets a unique ARID to post their commitment; coordinator polls each) to be carried as `response_arid` fields inside the initial request (Hubert pattern: each message tells the peer where to respond),
     - per-participant share ARIDs (where participants post signature shares) to be delivered as the next-hop `response_arid` inside the “signShare” request (again, the message carries the next ARID, not pre-agreed out-of-band).
   - Build initial GSTP “signCommit” request with parameters: group, targetDigest, minSigners/participant list, commitmentCollectArid, per-participant shareArid (individually encrypted to each participant inside the body).
   - Multicast pattern = DKG invite: per-participant response ARIDs encrypted under inner `recipient` assertion, then the whole GSTP request encrypted to all participants (multiple GSTP `recipient` assertions). No participant can see others’ ARIDs.
   - Post to each participant’s `send_to_arid` (from registry pending_requests). Preview mode (`--preview`) prints unsealed request for one participant; sealed mode posts with `--verbose` as desired.
   - Persist session state under `group-state/<group-id>/signing/<session-id>/start.json` (participant list, target digest, ARIDs).

2) ✅ **`frost sign participant receive` (participant)**
   - Pattern after `frost dkg participant invite receive`: supports Hubert fetch by ARID with optional `--timeout`, or direct envelope UR; `--timeout` requires storage, ARID inputs require storage, `--preview` not needed (non-mutating viewer).
   - Decrypt “signCommit” with owner private keys; validate function, session ID, group ID, minSigners bounds, and that caller’s XID is present in participant list.
   - Extract and display (with `--info`) key fields: coordinator, participant list, target digest/envelope summary, your commit `response_arid`, and your next-hop share ARID if present. `--no-envelope` mirrors invite receive behavior.
   - Persist request details to `group-state/<group-id>/signing/<session-id>/sign_receive.json` (source envelope UR, group/session IDs, coordinator, minSigners, sorted participants, response ARID, target UR); leave `registry.json` untouched. Do not print response ARID (write-once; persisted instead).

3) ✅ **`frost sign participant commit` (participant respond)**
   - Pattern after `frost dkg participant invite respond`: requires Hubert storage when posting; `--timeout` only with storage; MUST support `--preview` to show the unsealed response and dry-run (no state changes, no Hubert posts). Include an explicit rejection path (`--reject <reason>`) that posts a GSTP error/decline to the coordinator’s commit ARID and clears local signing state/listening ARID.
   - Load persisted details from `sign_receive.json` (group/session IDs, coordinator-provided response ARID, target UR, participants, coordinator) instead of re-fetching from Hubert; validate consistency. Coordinator must supply the commit `response_arid`; the participant generates only the next-hop ARID for the coordinator’s forthcoming signShare request.
   - Load participant key package (`contributions.key_package` from registry), run FROST signing part1 (`round1::commit`) to produce signing nonces + commitments; compute/record target digest from the request (use persisted target UR if present to avoid structural drift).
   - Build GSTP response (e.g., `signCommitResponse`) carrying group, session, commitments (JSON/CBOR), share-request ARID for the next hop, and targetDigest; include `peer_continuation` from request, sign with owner keys, and encrypt to coordinator.
   - Post to coordinator’s commit ARID (from request). Persist part1 state under `group-state/<group-id>/signing/<session-id>/commit.json` (nonces, commitments, target digest, session metadata, share-request ARID) and update `registry.json` to set `listening_at_arid` to the share-request ARID for the upcoming signShare step.
   - Demo notes: Show one participant preview; others post without preview.

4) ✅ **`frost sign coordinator collect` (coordinator)**
   - Collect all “signCommit” responses from the per-participant commit ARIDs recorded in `start.json` (no shared pending_requests; session-centric).
   - Validate session ID + sender, aggregate commitments, and persist per-participant commitments + share ARIDs under `group-state/<group-id>/signing/<session-id>/commitments.json`.
   - Build per-participant “signShare” GSTP request carrying aggregated commitments and each participant’s shareArid (where they will post their signature share). Pattern: 1-1 sealed delivery (no inner per-recipient ARIDs needed, just a single response ARID).
   - `--preview-share` prints one unsealed signShare request during collect.
   - Redundant fields removed: signShare carries only session, response_arid, and commitments (no group/minSigners/targetDigest).

5) ✅ **`frost sign participant share` (participant)**
   - Fetches signShare from `listening_at_arid`, validates session/minSigners/participants against persisted receive state, checks commitments against stored part1 values, builds signing package from the persisted target digest, and posts a signature share to the provided share ARID.
   - Persists `share.json` (signature share + commitments + finalize ARID) and sets `listening_at_arid` for the finalize hop.

6) ✅ **`frost sign coordinator finalize` (coordinator)**
   - Collect all signature shares from the share-collection ARID; validate and aggregate to the final `Signature::Ed25519`.
   - Verify the aggregated signature against the target digest and by attaching it to the target envelope with the group verifying key; abort if verification fails.
   - Persist final signature and session transcript under `group-state/<group-id>/signing/<session-id>/final.json`; print the `ur:signature/...` once after dispatch.
   - Post per-participant finalize packages (sealed to each participant’s provided ARID from signShareResponse) containing session ID and the signature shares needed for participants to independently recompute the signature (no aggregated signature sent).

7) **`frost sign participant attach` (participant)**
   - Fetch finalize package from personal ARID, reconstruct/verify the group signature using persisted session state plus provided shares/commitments, persist/print signature, attach it to the target envelope and verify locally using the group verifying key.

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

### Lessons Learned / Cleanup

- Sign state is now fully session-scoped: start/commit/collect use session IDs directly; group-level pending_requests are no longer used for signing phases.
- Avoid redundant fields: signShare drops group/minSigners/targetDigest; signCommitResponse drops group/targetDigest. Participants enforce invariants from the signed start state instead.
- Always transmit the literal target envelope, not a UR wrapper; digest validation relies on the envelope itself.
- `--no-envelope` was removed from receive flows; outputs now include info plus the bare session line for scripting.
- Signature share responses no longer echo `targetDigest`; session ID + persisted state already bind the target.
- Finalize packages to participants exclude the aggregated signature; participants recompute locally using provided shares. Coordinator verifies the aggregated signature on the target before dispatch and prints the UR after sends.

### Next Steps

- Extend `signShareResponse` to include a per-participant `response_arid` for the final hop so the coordinator can return aggregation material. ✅
- Replace `frost sign finalize` with `frost sign coordinator finalize` (coordinator): collect shares, verify/aggregate the joint signature, and post per-participant finalize packages (sealed) back to the `response_arid`, containing the session ID plus the commitments/signature shares needed for participants to deterministically recompute the signature (do not re-send target details already bound to the session). ✅
- Add a participant finalize command (`sign attach`) to fetch the finalize package, recompute/verify the signature locally using persisted session state + provided shares/commitments, persist/print the signature, and attach to the target envelope; add demo coverage.
- Add integration coverage for signShare/signFinalize, including failure cases (threshold mismatch, stale/incorrect session).

### Test Coverage

Add integration tests for:
- Round 2 message construction and distribution
- Finalize collect and group verifying key handling
- End-to-end signing flow (commit/share/finish)

### Demo Script Updates

- Demo now includes Round 2 collect, finalize send/respond/collect, group verifying key, and participant signShare responses.
- Next: extend `frost-demo.py` to cover sign finish once implemented.
