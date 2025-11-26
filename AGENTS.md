# Guidelines for Developers working with the `frost` crate

Always read this *entire* file before working in this crate.

## General Guidelines

- This crate is unreleased *de novo* development. Don't concern yourself with backward compatibility yet.

## Relevant Crates in This Workspace

- `bc-components`: Blockchain Commons Cryptographic Components
    - `XID`: eXtensible IDentifier
    - `ARID`: Apparently Random Identifier
- `bc-envelope`: Gordian Envelope
    - `Envelope`: Envelope Structure
    - `queries.rs`: Envelope Querying
- `dcbor`: Deterministic CBOR
    - `CBOR`: CBOR item
    - `Date`: Date Type
- `bc-xid`: XID (eXtensible IDentifiers)
    - `XIDDocument`: XID Document Structure
- `gstp`: Gordian Sealed Transaction Protocol
    - `SealedRequest`: Sealed Request Structure
    - `SealedResponse`: Sealed Response Structure
    - `SealedEvent`: Sealed Event Structure
- `hubert`: Gordian Hubert Protocol
- `bc-ur`: Blockchain Commons UR (Uniform Resources)
    - `UR`: UR Type
- `provenance-mark`: Provenance Marks
    - `ProvenanceMark`: Provenance Mark Type
    - `ProvenanceMarkGenerator`: Generator for Provenance Marks
    - `ProvenanceMarkResolution`: Resolution for Provenance Marks
- `research`: The Blockchain Commons Research Repository

## Tests

- Use the "expected text output rubric" approach for tests that compare text output.
- Use the `assert_actual_expected!` macro for comparing actual and expected text output in tests.
- Always use `indoc!` for multi-line expected text output in tests.

## Essential Hubert Knowledge (transport)

- Hubert is just a key/value transport keyed by ARID; always include and persist the ARID for the *next* hop when you expect a reply (store it as `listening_at_arid`, send it as `response_arid`).
- Field naming for ARID flow:
  - `response_arid`: tell the peer where to send their next message.
  - `send_to_arid`: coordinator records where to post to a participant.
  - `collect_from_arid`: coordinator records where to fetch a participant’s response.
  - `listening_at_arid`: local state for where *you* are listening next.
- Verbosity: use `--verbose` only when you want Hubert transfer logs; keep previews quiet.
- Previews: use `--preview` and pipe to `envelope format`; previews must not mutate local state or post to storage.
- Payload routing modes (transport-level concerns):
- Multicast preview/audit (e.g., preview invite): everyone can view the message, but per-recipient fields like response ARIDs must be individually encrypted under a `recipient` assertion on the inner assertion so other participants cannot see them.
  - Single-cast deliveries (e.g., Round 2 per-participant messages): the whole message is encrypted to one recipient; when wrapping per-recipient payloads, add a `recipient` assertion to the wrapped byte string so the coordinator can fan out later, but the payload itself stays inside the single-recipient envelope.

## Essential GSTP Knowledge (message encoding)

- Always include the request/response correlation: GSTP requests carry an ID; responses must use the same ID. Continuations (`peer_continuation`/`state`) let each side bounce back valid IDs and optional state.
- Use `SealedRequest::to_envelope_for_recipients` / `SealedResponse::to_envelope` with the sender’s signing keys and recipient encryption keys; `try_from_envelope` / `try_from_encrypted_envelope` on the receiver side.
- GSTP adds its own `recipient` encryption wrapper when you target specific recipients; do not confuse this with any inner `recipient` assertions you add for transport routing of individual fields (e.g., per-recipient response ARIDs).
- Function/parameter checks: compare against `Function::from("name")` rather than `.name()`; extract parameters via `extract_object_for_parameter`.
- Errors: `SealedResponse::error()` returns `Ok(envelope)` when an error is present—check it before reading `result()`.
- Dates/validity: include `valid_until` on requests when expiration matters; continuations are validated against expected IDs/dates.
- Byte data: wrap byte strings as CBOR byte strings (`CBOR::to_byte_string`) to avoid accidental arrays; unwrap with `try_leaf` + `CBOR::try_into_byte_string`.

## Important Notes

- `frost-demo.py` generates `demo-log.md`. When you enhance the tool, consider enhancing the `frost-demo.py` script to reflect those changes in the demo log then regenerate `demo-log.md`.
- Before you run `frost-demo.py`, install the latest build of the `frost` tool with `cargo install --path .`.
