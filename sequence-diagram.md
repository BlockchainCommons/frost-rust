# Frost Sequence Diagram

## Registry Setup

```mermaid
sequenceDiagram
    participant S as Signal
    actor A as Alice
    actor B as Bob
    actor C as Carol
    actor D as Dan

    A->>S: XIDDocument(A)
    B->>S: XIDDocument(B)
    C->>S: XIDDocument(C)
    D->>S: XIDDocument(D)

    S->>A: XIDDocument(B)<br/>XIDDocument(C)<br/>XIDDocument(D)
    S->>B: XIDDocument(A)<br/>XIDDocument(C)<br/>XIDDocument(D)
    S->>C: XIDDocument(A)<br/>XIDDocument(B)<br/>XIDDocument(D)
    S->>D: XIDDocument(A)<br/>XIDDocument(B)<br/>XIDDocument(C)
```

## Distributed Key Generation

```mermaid
sequenceDiagram
    participant S as Signal
    participant H as Hubert
    actor A as Alice<br/>(Coordinator)
    actor B as Bob
    actor C as Carol
    actor D as Dan

    note over A: dkg coordinator invite
    A->>H: dkgCoordinatorInvite(B, C, D)
    A->>S: invite ARID
    S->>B: invite ARID
    S->>C: invite ARID
    S->>D: invite ARID

    note over B: dkg participant invite receive
    H->>B: dkgCoordinatorInvite(B, C, D)

    note over C: dkg participant invite receive
    H->>C: dkgCoordinatorInvite(B, C, D)

    note over D: dkg participant invite receive
    H->>D: dkgCoordinatorInvite(B, C, D)

    note over B: dkg participant invite respond
    B->>H: dkgInviteResponse(B)

    note over C: dkg participant invite respond
    C->>H: dkgInviteResponse(C)

    note over D: dkg participant invite respond
    D->>H: dkgInviteResponse(D)

    note over A: dkg coordinator round1 collect
    H->>A: dkgInviteResponse(B)<br/>dkgInviteResponse(C)<br/>dkgInviteResponse(D)

    note over A: dkg coordinator round2 send
    A->>H: dkgRound2(B)<br/>dkgRound2(C)<br/>dkgRound2(D)

    note over B: dkg participant round2 respond
    H->>B: dkgRound2(B)
    B->>H: dkgRound2Response(B)

    note over C: dkg participant round2 respond
    H->>C: dkgRound2(C)
    C->>H: dkgRound2Response(C)

    note over D: dkg participant round2 respond
    H->>D: dkgRound2(D)
    D->>H: dkgRound2Response(D)

    note over A: dkg coordinator round2 collect
    H->>A: dkgRound2Response(B)<br/>dkgRound2Response(C)<br/>dkgRound2Response(D)

    note over A: dkg coordinator finalize send
    A->>H: dkgFinalize(B)<br/>dkgFinalize(C)<br/>dkgFinalize(D)

    note over B: dkg participant finalize respond
    H->>B: dkgFinalize(B)
    B->>H: dkgFinalizeResponse(B)

    note over C: dkg participant finalize respond
    H->>C: dkgFinalize(C)
    C->>H: dkgFinalizeResponse(C)

    note over D: dkg participant finalize respond
    H->>D: dkgFinalize(D)
    D->>H: dkgFinalizeResponse(D)

    note over A: dkg coordinator finalize collect
    H->>A: dkgFinalizeResponse(B)<br/>dkgFinalizeResponse(C)<br/>dkgFinalizeResponse(D)
```

## Signing

```mermaid
sequenceDiagram
    participant S as Signal
    participant H as Hubert
    actor A as Alice<br/>(Coordinator)
    actor B as Bob
    actor C as Carol
    actor D as Dan

    note over A: sign coordinator start
    A->>H: signCommit(B, C, D)
    A->>S: invite ARID
    S->>B: invite ARID
    S->>C: invite ARID
    S->>D: invite ARID

    note over B: sign participant receive
    H->>B: signCommit(B, C, D)
    note over B: sign participant commit
    B->>H: signCommitResponse(B)

    note over C: sign participant receive
    H->>C: signCommit(B, C, D)
    note over C: sign participant commit
    C->>H: signCommitResponse(C)

    note over D: sign participant receive
    H->>D: signCommit(B, C, D)
    note over D: sign participant commit
    D->>H: signCommitResponse(D)

    note over A: sign coordinator collect
    H->>A: signCommitResponse(B)<br/>signCommitResponse(C)<br/>signCommitResponse(D)
    A->>H: signShare(B)<br/>signShare(C)<br/>signShare(D)

    note over B: sign participant share
    H->>B: signShare(B)
    B->>H: signShareResponse(B)

    note over C: sign participant share
    H->>C: signShare(C)
    C->>H: signShareResponse(C)

    note over D: sign participant share
    H->>D: signShare(D)
    D->>H: signShareResponse(D)

    note over A: sign coordinator finalize
    H->>A: signShareResponse(B)<br/>signShareResponse(C)<br/>signShareResponse(D)
    A->>H: signFinalize(B)<br/>signFinalize(C)<br/>signFinalize(D)

    note over B: sign pariticipant attach
    H->>B: signFinalize(B)

    note over C: sign pariticipant attach
    H->>C: signFinalize(C)

    note over D: sign pariticipant attach
    H->>D: signFinalize(D)
```
