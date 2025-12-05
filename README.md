# `frost-hubert`

## A command-line tool for FROST threshold cryptography using Hubert as the distributed substrate

<!--Guidelines: https://github.com/BlockchainCommons/secure-template/wiki -->

### _by Wolf McNally & Christopher Allen_

**Status: Development Preview**

<img src="https://raw.githubusercontent.com/BlockchainCommons/Gordian/master/Images/logos/gordian-icon.png" width=200>

## Introduction

`frost-hubert` is a command-line tool (`frost-hubert`) for working with FROST (Flexible Round-Optimized Schnorr Threshold) signatures using the [Hubert](https://github.com/BlockchainCommons/hubert-rust) distributed coordination protocol. It implements distributed key generation (DKG), threshold signing of Gordian Envelopes, and participant registry management with cryptographically verified identities using [XID Documents](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2024-007-xid.md).

FROST enables threshold cryptography where a group of participants can collectively generate keys and create signatures without any single participant having access to the complete signing key. This tool implements the full workflow: from participant registration through distributed key generation to threshold signature creation.

### Key Features

- **Registry Management**: Record and verify participants using signed XID Documents
- **Distributed Key Generation**: Multi-round DKG protocol for generating threshold signing keys
- **Threshold Signing**: Create signatures requiring agreement from a threshold of participants
- **Multiple Storage Backends**: Support for Hubert server, Mainline DHT, IPFS, and hybrid storage
- **Parallel Operations**: Concurrent message sending ("puts") and collection ("gets") with progress display
- **Preview Mode**: Inspect protocol messages before sending
- **Gordian Envelope Integration**: All protocol messages use Gordian Envelope with encryption and authentication

## Installation

### From Source

```
# Clone the repository
git clone https://github.com/BlockchainCommons/frost-hubert-rust.git frost-hubert
cd frost-hubert
# Install the tool
cargo install --path .
```

Make sure your `~/.cargo/bin` directory is in your `PATH`.

## Usage

```
frost --help
```

### Command Structure

```
frost [--verbose] <COMMAND>

Commands:
  registry    Manage the FROST registry
  check       Check Hubert storage backend availability
  dkg         Distributed key generation operations
  sign        Threshold signing operations
```

### Registry Commands

Manage participants and the registry owner using signed XID Documents:

```
# Set the registry owner (requires private keys)
frost registry owner set [--registry <PATH>] <XID_DOCUMENT>

# Add a participant (uses public XID Document)
frost registry participant add [--registry <PATH>] <XID_DOCUMENT> [<PET_NAME>]
```

### DKG Commands

#### Coordinator Commands

```
# Send DKG invitations to participants
frost dkg coordinator invite send [OPTIONS] <PARTICIPANT>...
  --registry <PATH>           Registry file path
  --min-signers <N>           Minimum signers required (threshold)
  --charter <STRING>          Group charter/description
  --preview                   Preview without sending
  --parallel                  Use parallel operations
  --storage <BACKEND>         Storage backend: server|dht|ipfs|hybrid
  --host <HOST>               Storage server hostname
  --port <PORT>               Storage server port

# Collect Round 1 responses and send Round 2 requests
frost dkg coordinator round1 [OPTIONS] <GROUP_ID>
  --parallel                  Use parallel operations with progress display

# Collect Round 2 responses and send finalize requests
frost dkg coordinator round2 [OPTIONS] <GROUP_ID>
  --parallel                  Use parallel operations with progress display

# Collect finalize responses and output group public key
frost dkg coordinator finalize [OPTIONS] <GROUP_ID>
  --parallel                  Use parallel operations with progress display
```

#### Participant Commands

```
# Receive and view DKG invitation
frost dkg participant invite receive [OPTIONS] <UR:ARID|UR:ENVELOPE>
  --info                      Show invitation details
  --no-envelope               Parse as ARID only

# Respond to invitation (accept or reject)
frost dkg participant invite respond [OPTIONS] <UR:ARID|UR:ENVELOPE>
  --reject <REASON>           Reject with reason
  --preview                   Preview response

# Complete Round 1 (generate and send commitment)
frost dkg participant round1 [OPTIONS] <GROUP_ID>

# Complete Round 2 (generate and send proof)
frost dkg participant round2 [OPTIONS] <GROUP_ID>

# Finalize DKG (generate key package)
frost dkg participant finalize [OPTIONS] <GROUP_ID>
```

### Signing Commands

#### Coordinator Commands

```
# Send signing invitations
frost sign coordinator invite send [OPTIONS] <TARGET_ENVELOPE> <PARTICIPANT>...
  --session-id <ID>           Session identifier
  --parallel                  Use parallel operations

# Collect Round 1 commitments and send Round 2 requests
frost sign coordinator round1 [OPTIONS] <SESSION_ID>
  --parallel                  Use parallel operations

# Collect signature shares and combine into final signature
frost sign coordinator round2 [OPTIONS] <SESSION_ID>
  --parallel                  Use parallel operations
```

#### Participant Commands

```
# Receive and view signing invitation
frost sign participant receive [OPTIONS] <UR:ARID|UR:ENVELOPE>
  --info                      Show session details

# Generate and send commitment
frost sign participant round1 [OPTIONS] <SESSION_ID>

# Generate and send signature share
frost sign participant round2 [OPTIONS] <SESSION_ID>

# Validate final signature
frost sign participant finalize [OPTIONS] <SESSION_ID>
```

### Storage Backends

The tool supports multiple storage backends via Hubert:

- **server**: HTTP-based Hubert server
- **dht**: Mainline DHT (distributed hash table)
- **ipfs**: IPFS network
- **hybrid**: DHT + IPFS with fallback

Example with storage configuration:

```
frost dkg coordinator invite send \
  --storage server \
  --host localhost \
  --port 8080 \
  --min-signers 2 \
  Alice Bob Carol
```

### Parallel Operations

Use `--parallel` for concurrent message collection with progress display:

```
frost dkg coordinator round1 --parallel --storage server <GROUP_ID>
```

## Workflow Example

See the complete [demo log](demo-log.md) for a detailed walkthrough. The basic workflow:

1. **Provision Participants**: Create XID Documents for all participants
2. **Build Registry**: Owner creates registry and adds participants
3. **DKG Invitation**: Coordinator sends invites specifying threshold
4. **DKG Round 1**: Participants respond with commitments
5. **DKG Round 2**: Participants exchange proofs
6. **DKG Finalize**: Participants generate key packages
7. **Sign Invitation**: Coordinator initiates signing session
8. **Sign Round 1**: Participants generate nonce commitments
9. **Sign Round 2**: Participants create signature shares
10. **Combine Signature**: Coordinator combines shares into final signature

## Registry Storage

By default, commands store registry data in `registry.json` within the current working directory. The `--registry` option allows customization:

- Filename only: `--registry my-registry.json` (current directory)
- Directory: `--registry ./my-group/` (uses `registry.json` in that directory)
- Full path: `--registry /path/to/my-registry.json`

Re-running commands with identical arguments is idempotent.

## Related Projects

- [Hubert Protocol](https://github.com/BlockchainCommons/hubert-rust) - Distributed coordination substrate
- [Gordian Envelope](https://github.com/BlockchainCommons/bc-envelope-rust) - Smart document format
- [XID Specification](https://github.com/BlockchainCommons/bc-xid-rust) - eXtensible Identifier Documents
- [GSTP](https://github.com/BlockchainCommons/gstp-rust) - Gordian Sealed Transaction Protocol
- [zCash Foundation FROST Implementation](https://github.com/ZcashFoundation/frost) - FROST algorithm library
- [FROST Paper](https://eprint.iacr.org/2020/852.pdf) - Original FROST research

## Status - Development Preview

This tool is under active development. The protocol and command-line interface may change. Not recommended for production use.

The implementation follows the [FROST specification](https://datatracker.ietf.org/doc/draft-irtf-cfrg-frost/) with Ed25519 signatures. All protocol messages use Gordian Envelope for structure, encryption, and authentication.

We welcome feedback about the tool's functionality, API design, and use cases. Comments can be posted [to the Gordian Developer Community](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions).

See [Blockchain Commons' Development Phases](https://github.com/BlockchainCommons/Community/blob/master/release-path.md).

## Version History

### 0.1.0 - December 5, 2025
- Initial release

## Financial Support

`frost-hubert` is a project of [Blockchain Commons](https://www.blockchaincommons.com/). We are proudly a "not-for-profit" social benefit corporation committed to open source & open development. Our work is funded entirely by donations and collaborative partnerships with people like you. Every contribution will be spent on building open tools, technologies, and techniques that sustain and advance blockchain and internet security infrastructure and promote an open web.

To financially support further development of `frost-hubert` and other projects, please consider becoming a Patron of Blockchain Commons through ongoing monthly patronage as a [GitHub Sponsor](https://github.com/sponsors/BlockchainCommons). You can also support Blockchain Commons with bitcoins at our [BTCPay Server](https://btcpay.blockchaincommons.com/).

## Contributing

We encourage public contributions through issues and pull requests! Please review [CONTRIBUTING.md](https://github.com/BlockchainCommons/bc-rust/blob/master/CONTRIBUTING.md) for details on our development process. All contributions to this repository require a GPG signed [Contributor License Agreement](https://github.com/BlockchainCommons/bc-rust/blob/master/CLA.md).

### Discussions

The best place to talk about Blockchain Commons and its projects is in our GitHub Discussions areas.

[**Gordian Developer Community**](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions). For standards and open-source developers who want to talk about interoperable wallet specifications, please use the Discussions area of the [Gordian Developer Community repo](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions). This is where you talk about Gordian specifications such as [Gordian Envelope](https://github.com/BlockchainCommons/Gordian/tree/master/Envelope#articles), [bc-shamir](https://github.com/BlockchainCommons/bc-shamir), [Sharded Secret Key Reconstruction](https://github.com/BlockchainCommons/bc-sskr), and [bc-ur](https://github.com/BlockchainCommons/bc-ur) as well as the larger [Gordian Architecture](https://github.com/BlockchainCommons/Gordian/blob/master/Docs/Overview-Architecture.md), its [Principles](https://github.com/BlockchainCommons/Gordian#gordian-principles) of independence, privacy, resilience, and openness, and its macro-architectural ideas such as functional partition (including airgapping, the original name of this community).

[**Gordian User Community**](https://github.com/BlockchainCommons/Gordian/discussions). For users of the Gordian reference apps, including [Gordian Coordinator](https://github.com/BlockchainCommons/iOS-GordianCoordinator), [Gordian Seed Tool](https://github.com/BlockchainCommons/GordianSeedTool-iOS), [Gordian Server](https://github.com/BlockchainCommons/GordianServer-macOS), [Gordian Wallet](https://github.com/BlockchainCommons/GordianWallet-iOS), and [SpotBit](https://github.com/BlockchainCommons/spotbit) as well as our whole series of [CLI apps](https://github.com/BlockchainCommons/Gordian/blob/master/Docs/Overview-Apps.md#cli-apps). This is a place to talk about bug reports and feature requests as well as to explore how our reference apps embody the [Gordian Principles](https://github.com/BlockchainCommons/Gordian#gordian-principles).

[**Blockchain Commons Discussions**](https://github.com/BlockchainCommons/Community/discussions). For developers, interns, and patrons of Blockchain Commons, please use the discussions area of the [Community repo](https://github.com/BlockchainCommons/Community) to talk about general Blockchain Commons issues, the intern program, or topics other than those covered by the [Gordian Developer Community](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions) or the [Gordian User Community](https://github.com/BlockchainCommons/Gordian/discussions).

### Other Questions & Problems

As an open-source, open-development community, Blockchain Commons does not have the resources to provide direct support of our projects. Please consider the discussions area as a locale where you might get answers to questions. Alternatively, please use this repository's [issues](https://github.com/BlockchainCommons/bc-rust/issues) feature. Unfortunately, we can not make any promises on response time.

If your company requires support to use our projects, please feel free to contact us directly about options. We may be able to offer you a contract for support from one of our contributors, or we might be able to point you to another entity who can offer the contractual support that you need.

### Credits

The following people directly contributed to this repository. You can add your name here by getting involved. The first step is learning how to contribute from our [CONTRIBUTING.md](https://github.com/BlockchainCommons/bc-rust/blob/master/CONTRIBUTING.md) documentation.

| Name              | Role                     | Github                                           | Email                                 | GPG Fingerprint                                   |
| ----------------- | ------------------------ | ------------------------------------------------ | ------------------------------------- | ------------------------------------------------- |
| Christopher Allen | Principal Architect      | [@ChristopherA](https://github.com/ChristopherA) | \<ChristopherA@LifeWithAlacrity.com\> | FDFE 14A5 4ECB 30FC 5D22 74EF F8D3 6C91 3574 05ED |
| Wolf McNally      | Lead Researcher/Engineer | [@WolfMcNally](https://github.com/wolfmcnally)   | \<Wolf@WolfMcNally.com\>              | 9436 52EE 3844 1760 C3DC 3536 4B6C 2FCF 8947 80AE |

## Responsible Disclosure

We want to keep all of our software safe for everyone. If you have discovered a security vulnerability, we appreciate your help in disclosing it to us in a responsible manner. We are unfortunately not able to offer bug bounties at this time.

We do ask that you offer us good faith and use best efforts not to leak information or harm any user, their data, or our developer community. Please give us a reasonable amount of time to fix the issue before you publish it. Do not defraud our users or us in the process of discovery. We promise not to bring legal action against researchers who point out a problem provided they do their best to follow the these guidelines.

### Reporting a Vulnerability

Please report suspected security vulnerabilities in private via email to ChristopherA@BlockchainCommons.com (do not use this email for support). Please do NOT create publicly viewable issues for suspected security vulnerabilities.

The following keys may be used to communicate sensitive information to developers:

| Name              | Fingerprint                                       |
| ----------------- | ------------------------------------------------- |
| Christopher Allen | FDFE 14A5 4ECB 30FC 5D22 74EF F8D3 6C91 3574 05ED |

You can import a key by running the following command with that individual's fingerprint: `gpg --recv-keys "<fingerprint>"` Ensure that you put quotes around fingerprints that contain spaces.
