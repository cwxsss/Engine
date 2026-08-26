# Trusted Peer Identity Recovery Design

## Problem

Windows captures and dispatches clipboard entries, but an already paired HarmonyOS peer can answer with a generic `Rejected` acknowledgement. The installed HarmonyOS Engine accepts the clipboard wire protocol and only rejects before ingest when the remote endpoint fingerprint is absent from the active member roster or the frame cannot be decoded. The two clients currently use the same clipboard frame and header format, so the observed failure is consistent with an incomplete or stale sponsor-side admission record after a restart or upgrade.

Users must not need to clear both clients before adding or recovering a Windows device. Recovery must not weaken the endpoint fingerprint boundary or accept a device solely because it presents a known display name or device id.

## Scope

- Windows and HarmonyOS Engine runtimes only.
- Preserve paired-space data across process restarts and compatible upgrades.
- Recover a previously authenticated admission transaction when either sponsor or joiner restarts before all durable roster state is converged.
- Keep truly unknown, removed, or unverifiable endpoint fingerprints rejected.
- Produce a typed local failure reason that distinguishes unknown fingerprint, admission rejection, frame decode failure, missing inbound consumer, and application rejection.

Linux-specific behavior and automatic trust of a rotated identity without cryptographic continuity are out of scope.

## Design

### Durable admission convergence

The pairing invitation and admission transaction remain available until both sides can prove the resulting membership is durable. Startup recovery resumes any prepared, committed, or completion-pending transaction and replays idempotent sponsor/joiner completion messages. Replays must converge to the same member device id and identity fingerprint; conflicting data remains an error.

The sponsor member repository is the source used by the Iroh clipboard receiver. A recovered admission is complete only when the effective active member row contains the endpoint fingerprint authenticated during pairing. A regression test will restart both roles at each durable boundary and then send a real clipboard frame through the receiver adapter.

### Structured rejection

The receiver will retain the existing one-byte acknowledgement for compatibility. Internally it will record a typed rejection cause and expose it through dispatch diagnostics/delivery state when the peer supports the current protocol. Compatibility senders continue to see a generic rejection. No detailed secret, key, or raw fingerprint is sent over the wire or surfaced in UI logs.

### Security boundary

Clipboard traffic is accepted only when the remote Iroh public key derives to the fingerprint of an effective active member and that member is admitted by current space protection. Device id, display name, LAN address, and historical membership are never sufficient substitutes. If durable replay cannot establish that invariant, the operation remains rejected and requires an explicit invitation-based re-pair, without clearing unrelated spaces or devices.

## Error handling

- Retry idempotent admission replay after restart and transient storage/network failures.
- Do not retry corrupt or conflicting admission facts as if they were transient.
- A generic peer rejection remains retryable for transport scheduling, but it does not silently mutate trust.
- Diagnostics report stable categories without exposing secure material.

## Verification

- Unit tests for typed receiver rejection categories.
- Admission recovery tests covering sponsor and joiner restart at every durable phase.
- End-to-end test: recovered active member sends a real clipboard frame and receives `Accepted`.
- Negative tests: unknown fingerprint, removed member, conflicting replay, and malformed frame remain rejected.
- Windows Engine test suite and relevant Iroh integration tests pass.

