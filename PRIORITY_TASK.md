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

| File                                   | Lines | Recommendation                   |
| -------------------------------------- | ----- | -------------------------------- |
| `src/cmd/sign/participant/finalize.rs` | ✅     | Refactored into helper functions |
| `src/cmd/sign/coordinator/invite.rs`   | ✅     | Refactored into helper functions |
| `src/cmd/dkg/coordinator/round1.rs`    | ✅     | Refactored into helper functions |

**`sign/participant/finalize.rs` refactoring (completed):**
- `exec()` reduced from ~220 lines to ~75 lines
- Extracted validation helpers: `validate_session_state()`, `validate_share_state()`, `validate_finalize_request()`, `validate_signature_shares()`
- Extracted fetch helper: `fetch_finalize_request()`
- Extracted FROST aggregation: `aggregate_and_verify_signature()`, `update_registry_verifying_key()`

**`sign/coordinator/invite.rs` refactoring (completed):**
- `exec()` reduced from ~170 lines to ~65 lines
- Introduced `SessionArids` struct to manage session/start/commit/share ARIDs
- Introduced `SignInviteContext` struct to bundle request-building parameters
- Extracted helpers: `validate_coordinator()`, `gather_recipient_documents()`, `build_sign_invite_request()`, `build_session_state_json()`, `persist_session_state()`, `post_to_hubert()`

**`dkg/coordinator/round1.rs` refactoring (completed):**
- `exec()` reduced from ~100 lines to ~55 lines; removed `#[allow(clippy::too_many_arguments)]`
- Introduced `Round1Context` struct to bundle runtime/client/registry/owner parameters
- Introduced type aliases `Round1Package` and `NextResponseArid` to reduce type complexity
- Collection phase: `collect_round1_responses()`, `fetch_all_round1_packages()`, `persist_round1_packages()`, `update_pending_for_round2()`
- Dispatch phase: `dispatch_round2_requests()`, `build_round2_participant_info()`, `update_pending_for_round2_collection()`
- Response handling: `validate_round1_response()`, `extract_round1_package()`
- Output: `print_summary()`

---

### 4. Low Priority: Documentation

- Add doc comments to internal helper functions (especially envelope parsing logic)
- Document the GSTP request/response flow at each protocol stage
- Add module-level documentation explaining coordinator vs participant roles

---

### 5. Low Priority: Type Organization ✅

**Cross-cutting utilities consolidated:**
- Created `src/cmd/common.rs` with shared utilities:
  - `parse_arid_ur()` — ARID UR parsing
  - `OptionalStorageSelector` — storage backend CLI args
  - `signing_key_from_verifying()` — FROST verifying key conversion
  - `group_state_dir()` — group state directory path
- `src/cmd/dkg/common.rs` now re-exports from `cmd::common` and keeps DKG-specific utilities (participant resolution, group building, name formatting)
- `src/cmd/sign/common.rs` updated to use `group_state_dir()` from `cmd::common`, contains signing-specific `signing_state_dir()` functions

**State structs (`ReceiveState`, `ShareState`, etc.):**
- Kept in their respective files (finalize.rs, round1.rs, round2.rs)
- These are phase-specific with different fields per phase; moving them would add indirection without benefit
- Each file now has organized sections (Context/result types, Validation, etc.) from earlier refactoring
