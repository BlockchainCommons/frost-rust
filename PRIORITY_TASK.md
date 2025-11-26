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

3. **Storage Backends**
   - Hubert server (HTTP)
   - Mainline DHT
   - IPFS
   - Hybrid (DHT + IPFS)

4. **Demo Script** (`frost-demo.py`)
   - Provisions 4 participants (Alice, Bob, Carol, Dan)
   - Builds registries
   - Creates and responds to DKG invites via Hubert

## Where the Demo Stops

The `demo-log.md` ends after all participants respond to the invite. The DKG Round 1 packages are generated and persisted locally (`group-state/<group-id>/round1_secret.json`, `round1_package.json`), but the protocol is incomplete.

## Next Steps (Priority Order)

### 1. Coordinator Collects Round 1 Packages

**Command:** `frost dkg round1 collect`

The coordinator (Alice) needs to:
- Fetch all participant responses from Hubert using their assigned response ARIDs
- Validate each response (GSTP response, signature, group membership)
- Extract Round 1 packages from successful responses
- Store the collected packages locally

**Why first:** Without collecting Round 1, the coordinator cannot proceed to Round 2.

### 2. Coordinator Sends Round 2 Requests

**Command:** `frost dkg round2 send`

The coordinator:
- Constructs Round 2 messages for each participant pair
- Creates a new GSTP request with all Round 1 packages
- Seals and sends to Hubert with per-participant encrypted response ARIDs

### 3. Participants Complete Round 2

**Command:** `frost dkg round2 respond`

Each participant:
- Fetches Round 2 request
- Runs `frost_ed25519::keys::dkg::part2` with their Round 1 secret and all Round 1 packages
- Generates Round 2 packages (one per other participant)
- Persists `round2_secret.json`
- Posts encrypted Round 2 packages back to coordinator

### 4. Coordinator Collects and Distributes Round 2

**Command:** `frost dkg round2 collect` and `frost dkg finalize send`

The coordinator:
- Collects all Round 2 packages
- Redistributes each participant's incoming Round 2 packages to them

### 5. Participants Finalize Key Generation

**Command:** `frost dkg finalize`

Each participant:
- Runs `frost_ed25519::keys::dkg::part3` with their Round 2 secret and incoming packages
- Produces `KeyPackage` and `PublicKeyPackage`
- Stores `key_package.json`
- Updates group status to `Complete`

### 6. Group Status and Listing

**Commands:**
- `frost group list` - List all groups in registry with status
- `frost group info <GROUP_ID>` - Show group details, participants, coordinator, signing threshold

### 7. Threshold Signing

Once key generation is complete:
- `frost sign start` - Coordinator initiates signing session
- `frost sign commit` - Participant sends commitment
- `frost sign contribute` - Participant sends signature share
- `frost sign finish` - Coordinator aggregates shares into final signature

## Implementation Notes

### GroupRecord Enhancements

The `ContributionPaths` structure is ready for Round 2:
```rust
pub struct ContributionPaths {
    pub round1_secret: Option<String>,
    pub round1_package: Option<String>,
    pub round2_secret: Option<String>,  // Ready but unused
    pub key_package: Option<String>,    // Ready but unused
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
- Round 1 collection from Hubert
- Round 2 message construction and distribution
- Key package generation
- End-to-end signing flow

### Demo Script Updates

After implementing each phase, extend `frost-demo.py` to demonstrate the full flow from invite through signing.

## Lower Priority Enhancements

- **Timeout handling** for unresponsive participants
- **Resharing** when participants need to be replaced
- **Key rotation** for long-lived groups
- **Audit logging** of all DKG and signing operations
- **Recovery mode** if a participant loses state mid-protocol
