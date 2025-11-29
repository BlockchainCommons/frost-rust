# frost

`frost` is a lightweight command-line helper for working with Flexible Round-Optimized Schnorr Threshold (FROST) participants within the Blockchain Commons toolchain. It validates and records participants and the registry owner from signed `ur:xid` documents.

## Usage

```
frost registry participant add [--registry <PATH>] <XID_DOCUMENT> [<PET_NAME>]
frost registry owner set [--registry <PATH>] <XID_DOCUMENT>
frost dkg coordinator invite send [--registry <PATH>] [--min-signers <N>] [--charter <STRING>] [--preview] [--storage <BACKEND> --host <HOST> --port <PORT>] <PARTICIPANT>...
frost dkg participant invite receive [--registry <PATH>] [--timeout <SECONDS>] [--no-envelope] [--info] [--sender <SENDER>] [--storage <BACKEND> --host <HOST> --port <PORT>] <UR:ARID|UR:ENVELOPE>
frost dkg participant invite respond [--registry <PATH>] [--timeout <SECONDS>] [--response-arid <UR:ARID>] [--preview] [--reject <REASON>] [--sender <SENDER>] [--storage <BACKEND> --host <HOST> --port <PORT>] <UR:ARID|UR:ENVELOPE>
```

- `XID_DOCUMENT` must be a valid `ur:xid` string representing an `XIDDocument` that is signed by its inception key.
- `PET_NAME` is an optional human-readable alias. If provided it must be unique within the current directory's `registry.json` registry.
- `--registry <PATH>` overrides where the participant registry is stored. Provide just a filename to keep it in the current directory, a directory path ending in `/` to use the default `registry.json` within that directory, or a path that already contains a filename (absolute or relative) to use it verbatim.

By default commands store registry data in `registry.json` within the current working directory, creating the file if it does not exist. Re-running the same command with identical arguments is idempotent.

The `registry owner set` command records an owner entry whose `XIDDocument` must include private keys; it fails if an owner already exists with different keys. The `dkg coordinator invite send` command seals a DKG invite for the selected participants; without Hubert parameters it prints the sealed (or `--preview`) envelope UR for inspection, and with Hubert parameters it stores the sealed invite and prints only the ARID to share out-of-band. The `dkg participant invite receive` command decrypts and validates a sealed invite from Hubert or a provided envelope and can optionally print summary info. The `dkg participant invite respond` command composes a response to an invite; without Hubert parameters it prints the sealed or preview response envelope, and with Hubert parameters it posts the sealed response without printing an ARID because the request/response flow already carries it.

## License

BSD 2-Clause Plus Patent License. See `LICENSE` in the repository root.
