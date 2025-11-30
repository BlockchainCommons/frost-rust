# Priority Tasks for `frost` Crate

## Code Quality Improvements

### 1. High Priority: Extract Duplicate Utility Functions ✅ COMPLETED

All duplicate utility functions have been consolidated:

#### a) `parse_arid_ur` ✅
- Canonical location: `src/cmd/dkg/common.rs`
- Removed duplicates from `dkg/coordinator/finalize.rs` and `sign/coordinator/invite.rs`

#### b) `group_state_dir` ✅
- Added to `src/cmd/dkg/common.rs`
- Removed 6 duplicates from DKG modules

#### c) `signing_state_dir` and `signing_state_dir_for_group` ✅
- Created `src/cmd/sign/common.rs` with both functions
- Removed 9 duplicates from sign modules

#### d) `signing_key_from_verifying` ✅
- Added to `src/cmd/dkg/common.rs`
- Removed 4 duplicates (from both DKG and sign modules)

#### e) `group_participant_from_registry` ✅
- Made public in `src/cmd/dkg/common.rs`
- Removed duplicate from `dkg/participant/round1.rs`

---

### 2. Medium Priority: Naming Consistency

#### a) `DkGProposedParticipant` casing ✅
The struct name uses unusual casing (`DkG`). Standard Rust style would be `DkgProposedParticipant`.

**Files affected**:
- `src/dkg/proposed_participant.rs`
- `src/dkg/group_invite.rs`
- `src/lib.rs` (re-export)

#### b) Variable naming inconsistency ✅

**`xid_document` vs `xid_doc`**: Fixed. Renamed methods and variables to use consistent `xid_document` prefix:
- `xid_doc_ur()` → `xid_document_ur()`
- `xid_doc_envelope()` → `xid_document_envelope()`
- Local variable `xid_doc_envelope` → `xid_document_envelope`

**`response_arid` vs `next_response_arid`**: These are correctly named for their distinct semantic roles:
- `response_arid` — the ARID where this party should post their response
- `next_response_arid` — the ARID this party includes to tell the other side where to send the *next* message

No change needed; the naming reflects the protocol's message flow.

---

### 3. Medium Priority: Decompose Long Functions

Several `exec()` methods exceed 100 lines and mix multiple concerns:

| File                                   | Lines | Recommendation                                            |
| -------------------------------------- | ----- | --------------------------------------------------------- |
| `src/cmd/sign/participant/finalize.rs` | ~220  | Extract: validation, FROST aggregation, state persistence |
| `src/cmd/sign/coordinator/invite.rs`   | ~170  | Extract: request building, state persistence, sending     |
| `src/cmd/dkg/coordinator/round1.rs`    | ~100  | Extract: collection phase, dispatch phase                 |

---

### 4. Low Priority: Documentation

- Add doc comments to internal helper functions (especially envelope parsing logic)
- Document the GSTP request/response flow at each protocol stage
- Add module-level documentation explaining coordinator vs participant roles

---

### 5. Low Priority: Type Organization

- `ReceiveState`, `ShareState`, and similar structs are defined at file bottom; consider grouping or extracting to a `state.rs` module
- Evaluate whether `src/cmd/common.rs` should exist for cross-cutting utilities shared by both `dkg` and `sign` subcommands
