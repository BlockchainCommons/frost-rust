## Make existing commands consistent

- `invite send` replaces `invite compose`.
- `invite view` is no longer necessary.

```
# Just composes the sealed invite and prints its envelope UR:

frost dkg invite send <registry-info> <config-info> <participants>

# The output of this command could be posted using the `hubert put` command to store it in Hubert for retrieval by the participants.
```

```
# Just composes the unsealed invite and prints its envelope UR:

frost dkg invite send --unsealed <registry-info> <config-info> <participants>

# This version is unsealed, so it's not suitable for posting to Hubert, but it can be inspected directly.
```

```
# Composes the sealed invite, stores it in Hubert, and prints the invite ARID UR:

frost dkg invite send <registry-info> <config-info> <hubert info> <participants>

# Note the `--unsealed` flag is invalid in this case.
```

```
frost dkg invite receive <registry-info> <hubert-info> <invite-arid> [--timeout <seconds>] [--no-envelope] [--info]

# This command retrieves the sealed invite from Hubert using the provided ARID, decrypts and verifies it, and prints the invite envelope (unless --no-envelope) and invite details (if --info).
```

```
frost dkg invite receive <registry-info> <invite-envelope>

# This command processes the provided invite envelope directly, decrypts and verifies it, and prints the invite details.
```

```
# Compose a response to an invite, either accepting it (default) or rejecting it with a reason:

frost dkg invite respond <registry-info> <invite-envelope> [--reject <reason>]

# This command composes a response to the provided invite envelope, either accepting it (default) or rejecting it with the provided reason, and prints the sealed response envelope UR.
# This version can be posted to Hubert for retrieval by the coordinator. The coordinator knows where to find it because the invite included a reply ARID for the participant to use.
```

```
# Compose a response to an invite, either accepting it (default) or rejecting it with a reason, and print the unsealed response envelope UR:

frost dkg invite respond <registry-info> <invite-envelope> [--reject <reason>] --unsealed

# This command composes a response to the provided invite envelope, either accepting it (default) or rejecting it with the provided reason, and prints the unsealed response envelope UR for inspection.
# This version is not suitable for posting to Hubert.
```

```
# Compose a response to an invite, either accepting it (default) or rejecting it with a reason, and store it in Hubert.

frost dkg invite respond <registry-info> <hubert-info> <invite-envelope> [--reject <reason>]

# The response ARID for the coordinator's next message is included in the response envelope, and therefore does not need to be printed separately.
```

## Plan for `dkg invite respond` and group tracking

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
