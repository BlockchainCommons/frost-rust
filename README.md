# frost

`frost` is a lightweight command-line helper for working with Flexible Round-Optimized Schnorr Threshold (FROST) participants within the Blockchain Commons toolchain. It currently focuses on validating and recording participants from signed `ur:xid` documents.

## Usage

```
frost participant add <XID_DOCUMENT> [<PET_NAME>]
```

- `XID_DOCUMENT` must be a valid `ur:xid` string representing an `XIDDocument` that is signed by its inception key.
- `PET_NAME` is an optional human-readable alias. If provided it must be unique within the current directory's `particiapants.json` registry.

The command stores the participant details in `particiapants.json` within the current working directory, creating the file if it does not exist. Re-running the same command with identical arguments is idempotent.

## License

BSD 2-Clause Plus Patent License. See `LICENSE` in the repository root.
