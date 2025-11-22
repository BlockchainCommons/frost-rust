# Guidelines for Developers working with the `frost` crate

## Important Crates in This Workspace

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
