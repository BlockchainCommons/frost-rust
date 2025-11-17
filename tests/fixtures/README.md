# XID Fixtures

Test fixtures in this directory were generated with the `envelope` CLI, which is already available elsewhere in this workspace (`bc-envelope-cli`). Commands follow the documented flows in `bc-envelope-cli/docs/XID.md`:

```sh
# Alice: signed inception document without embedded private keys
read -r ALICE_PRV ALICE_PUB <<< "$(envelope generate keypairs)"
SIGNED_ALICE=$(envelope xid new "$ALICE_PRV" --private omit --nickname "Alice" --sign inception)
printf "%s" "$SIGNED_ALICE" > alice_signed_xid.txt

# Bob: signed and unsigned variants
read -r BOB_PRV BOB_PUB <<< "$(envelope generate keypairs)"
SIGNED_BOB=$(envelope xid new "$BOB_PRV" --private omit --nickname "Bob" --sign inception)
UNSIGNED_BOB=$(envelope xid new "$BOB_PUB" --private omit --nickname "Bob")
printf "%s" "$SIGNED_BOB" > bob_signed_xid.txt
printf "%s" "$UNSIGNED_BOB" > bob_unsigned_xid.txt
```

The accompanying `*_prvkeys` and `*_pubkeys` files capture the keypairs used to mint the fixtures and can be regenerated in the same manner if needed. All strings are stored without trailing newlines for easier comparison inside tests.
