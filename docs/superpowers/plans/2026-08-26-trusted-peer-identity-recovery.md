# Trusted Peer Identity Recovery Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: Use subagent-driven-development to execute this plan task-by-task. Follow test-driven-development for every production change and verification-before-completion before delivery.

**Goal:** Ensure a securely paired Windows/HarmonyOS peer remains cryptographically admitted after process restarts, can exchange a real clipboard frame, and reports locally distinguishable rejection reasons without weakening the one-byte wire acknowledgement or trust checks.

**Architecture:** Keep the existing durable admission replay as the source of identity convergence. Add regression coverage at the public Engine boundary that restarts both peers and sends actual clipboard content. Improve local diagnostics by classifying a received generic `Rejected` acknowledgement according to the receiver-observable failure path while preserving wire compatibility; no device-name, device-id, address, or history-only fallback is allowed.

**Tech Stack:** Rust, Tokio, Iroh, Cargo integration tests, existing Engine HarmonyOS N-API packaging.

---

### Task 1: Lock the restarted sponsor/joiner clipboard path with a regression test

**Files:**
- Modify: `crates/uc-engine/tests/space_membership_auto_pairing_e2e.rs`
- Reference: `crates/uc-application/src/space/convergence/admission/tests.rs`

**Step 1: Write the failing regression test**

Add a focused test named `restarted_sponsor_accepts_clipboard_from_recovered_joiner` that:

1. creates a sponsor and joiner using persistent `DeviceHarness` profiles;
2. completes pairing and records both stable device IDs;
3. shuts down both engines;
4. restarts both from the same profiles and invokes recovery;
5. waits for member convergence;
6. sends a real text clipboard frame from joiner to sponsor and verifies durable receipt;
7. sends a real frame in the reverse direction;
8. asserts no reset or re-pair step occurred.

Reuse `create_space`, `join_through`, `recover`, `wait_for_converged_members_with_diagnostics`, and `send_and_verify`; do not mock the receiver or fingerprint repository.

**Step 2: Run the test and record RED or characterization evidence**

Run:

```powershell
cargo test -p uc-engine --test space_membership_auto_pairing_e2e restarted_sponsor_accepts_clipboard_from_recovered_joiner -- --nocapture
```

Expected: fail with a rejected delivery if the production gap remains. Rebase the plan/docs commits onto the Windows-pinned `v1.1.0-rc.6` commit `f449698b6e96e5d99549c3fdd076dcd8e68118ce` before final verification. If the test passes there, retain it as a characterization regression and do not invent a production change; the already-landed durable admission replay is the fix that HarmonyOS must consume.

**Step 3: If RED, make the minimum secure recovery correction**

Only if Step 2 reproduces the failure, modify the smallest responsible code under:

- `crates/uc-application/src/space/convergence/admission/`
- or its durable repository adapter under `crates/uc-infra/src/db/repositories/`

The correction must replay the paired member ID and authenticated fingerprint atomically/idempotently. It must not accept a fingerprint from display name, device ID, address, stale history, or an unauthenticated clipboard frame.

**Step 4: Run GREEN and adjacent recovery tests**

Run:

```powershell
cargo test -p uc-engine --test space_membership_auto_pairing_e2e restarted_sponsor_accepts_clipboard_from_recovered_joiner -- --nocapture
cargo test -p uc-application sponsor_recovery_finishes_durable_candidate_after_restart -- --nocapture
cargo test -p uc-application sponsor_accepts_next_candidate_after_recovery_converges -- --nocapture
```

Expected: all selected tests pass and each command runs at least one test.

**Step 5: Commit**

```powershell
git add crates/uc-engine/tests/space_membership_auto_pairing_e2e.rs crates/uc-application/src/space/convergence/admission crates/uc-infra/src/db/repositories
git commit -m "test: cover clipboard delivery after admission recovery"
```

Stage only paths actually changed.

### Task 2: Add typed local rejection diagnostics without changing the wire protocol

**Files:**
- Modify: `crates/uc-core/src/ports/clipboard/sync_dispatch.rs`
- Modify: `crates/uc-infra/src/network/iroh/clipboard_dispatch_adapter.rs`
- Modify: `crates/uc-infra/src/network/iroh/clipboard_receiver_adapter.rs`
- Modify only if required by compilation: call sites matching `ClipboardDispatchError::PeerRejected`

**Step 1: Write failing unit tests**

Extend receiver/dispatch adapter tests to distinguish at least:

- unknown authenticated fingerprint;
- known but not currently admitted member;
- malformed clipboard frame;
- application-side rejection/no subscriber;
- unknown acknowledgement byte.

The public error must expose a stable typed category plus safe detail. Tests must assert the category rather than matching the phrase `Rejected ack`.

**Step 2: Run tests to verify RED**

Run:

```powershell
cargo test -p uc-infra clipboard_receiver_adapter -- --nocapture
cargo test -p uc-infra clipboard_dispatch_adapter -- --nocapture
```

Expected: compilation or assertions fail because typed categories do not yet exist.

**Step 3: Implement the minimum typed model**

Introduce a small `PeerRejectionReason` enum in the clipboard dispatch port and carry it through `ClipboardDispatchError::PeerRejected`. Preserve the existing one-byte `AckCode` wire format. Because the sender cannot derive a detailed receiver reason from one byte, attach detailed receiver categories to local tracing/diagnostics and use `UnspecifiedByPeer` for the sender-visible generic rejection; use `UnknownAck` for an invalid byte. Never include secrets, invitation material, or full fingerprints in logs.

**Step 4: Run GREEN and blast-radius checks**

Run:

```powershell
cargo test -p uc-infra clipboard_receiver_adapter -- --nocapture
cargo test -p uc-infra clipboard_dispatch_adapter -- --nocapture
cargo test -p uc-core
cargo check --workspace --all-targets
```

Expected: all pass; existing call sites compile against the typed variant.

**Step 5: Commit**

```powershell
git add crates/uc-core/src/ports/clipboard/sync_dispatch.rs crates/uc-infra/src/network/iroh/clipboard_dispatch_adapter.rs crates/uc-infra/src/network/iroh/clipboard_receiver_adapter.rs
git commit -m "fix: classify clipboard peer rejections"
```

Stage any additional compile-fix paths explicitly, never `git add .`.

### Task 3: Verify the Engine revision for Windows and HarmonyOS consumption

**Files:**
- Verify: `tests/hosts/ohos/build-emulator.sh`
- Verify: `bindings/uc-ohos-napi/ohos/index.d.ts`
- Modify only if a public binding changed: generated/declared HarmonyOS type surface

**Step 1: Run complete relevant Engine suites**

Run:

```powershell
cargo test -p uc-application
cargo test -p uc-infra
cargo test -p uc-engine --test space_membership_auto_pairing_e2e
cargo check --workspace --all-targets
```

Expected: all pass with zero test failures.

**Step 2: Validate release metadata/build inputs**

Run:

```powershell
node scripts/architecture/check-engine-repository.mjs
node scripts/release/verify-version.mjs v1.1.0-rc.6 uc-engine
git status --short
git rev-parse HEAD
```

Expected: architecture and version checks pass; working tree contains only intentional commits. Record the resulting 40-character source commit for the HarmonyOS vendor task.

**Step 3: Commit metadata only if required**

Do not create or mutate an immutable release tag during implementation. If a binding declaration had to change, commit it separately:

```powershell
git add bindings/uc-ohos-napi/ohos/index.d.ts tests/hosts/ohos
git commit -m "chore: align HarmonyOS engine binding metadata"
```

### Task 4: Review and hand off the verified Engine commit

**Step 1: Inspect change scope**

```powershell
git diff f449698b6e96e5d99549c3fdd076dcd8e68118ce...HEAD --stat
git diff f449698b6e96e5d99549c3fdd076dcd8e68118ce...HEAD
git status --short
```

Expected: only admission regression coverage, secure recovery correction if required, and typed diagnostics are present.

**Step 2: Request code review**

Review specifically for cryptographic trust weakening, wire compatibility, retry semantics, and false-positive acceptance of removed/unknown peers. Resolve every must-fix finding and rerun Task 3.

**Step 3: Produce handoff evidence**

Record the Engine source commit, test commands/results, and the exact HarmonyOS files/artifacts that must be regenerated. Do not claim device success from build evidence alone.
