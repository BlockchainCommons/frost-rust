Plan for `dkg invite respond` and group tracking

1) Rename session ID to group ID
   - Throughout CLI, registry, and envelopes, treat the invite’s session ID as `group_id`. Update terminology accordingly.

2) Registry schema update
   - Add a `groups` map keyed by `group_id` storing charter, min_signers, participants (pet name + xid), coordinator, status, and local contribution paths (round1/round2/key package). Provide helpers to load/save these group records.

3) Deterministic participant identifiers
   - Use lexicographic ordering of participant XIDs to assign FROST identifiers; derive the current participant’s identifier index from this ordering and reuse it consistently in all DKG rounds.

4) Implement `dkg invite respond`
   - Reuse invite parsing/validation; persist the group config into the registry under `group_id`.
   - `--accept` (default): run `frost_ed25519::keys::dkg::part1` with identifier/total/min, store the round1 SecretPackage locally (per group/participant), record its path in the registry, and send a signed accept envelope (identifier index + round1 package) to the reply ARID via Hubert. Include a new response ARID (for the coordinator’s next message) in every Hubert post because further exchanges are expected.
   - `--reject`: mark group status reject and send a reject envelope, also including a new response ARID for follow-up if needed.
   - Provide a way to view/print the composed response envelope before sending (similar to invite compose), for inspection or offline delivery.

5) Future follow-ups (out of scope to code now)
   - Coordinator commands to collect responses, run part2, distribute round2 packages, and finalize group keys; participant commands to consume round2 and produce final key packages, all keyed by `group_id` and using the same XID ordering.
   - All Hubert messages that expect further replies must carry a fresh response ARID. Maintain the GSTP contract: the invite is a GSTP `Request`, so invite responses must be `Response` envelopes carrying the same GSTP request ID (distinct from the group/session ID).
