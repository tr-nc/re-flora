# Terminal visibility evidence and the legacy e2 capture

## Reproduction and cause

On base `618203af`, the scope-v1 `terrain-edits-inflight-capture` command at spacing 32,
`--environment-irradiance-capture-target e2 --auto-exit 15`, exits 101 with
`DDGI visibility sample evidence owner version is inconsistent`. Reducing only auto-exit to
one second reproduces the same failure. Raw GPU lanes 13..30 for first probe 3584, count 512,
geometry 3, epoch 0 are:

```text
[2,512,114,0,398,0, 2,512,0,512,0,0, 0,0,0,0, 0,0]
```

Ranked hypotheses were missing terminal-path evidence, a genuinely wrong owner version, and
reset/readback loss. The lanes discriminate them: visibility history has the correct owner mask
and exactly 512 Retain results, but all four sample lanes are zero. The terminal store returns
without executing the sample loop, which was the only writer of the sample-policy owner bit.
This is missing invocation evidence, not permission to accept an arbitrary zero owner mask.

## Owner interface and contract

A separate `begin evidence` call at the shader entry would leave callers responsible for pairing
it with every exit. Instead, the existing visibility owner commits both terminal and fresh stores
through `ddgiOwnerStoreVisibilityResult`. Its `ddgiRecordVisibilityFilterResult` produces history
and the sample-policy owner witness together. Sample counts still come only from real ray-loop
calls; per-sample owner versions are ORed, never overwritten by the completion witness.

- Every completed, capture-enabled visibility result records the current sample-policy owner,
  including Retain and invalid-fresh Replace with zero rays.
- Only the elected probe lane records evidence. Disabled evidence writes nothing.
- Fresh/mixed batches retain the existing Accept/Reject partitions, whole-probe sample bounds,
  exact Blend-retention witnesses and wrong/mixed-owner rejection.
- Radiance-only work still does not dispatch visibility or write any visibility evidence.

`IDdgiFilterEvidenceSink` is a statically specialized storage adapter, not a policy extension or
runtime registry. The GPU adapter retains the existing atomic lanes; Slang CPU tests use an array
adapter to execute the **same producer functions**, including their gating and owner-bit encoding.
The minimized one-Retain test exited 2 before the fix. Tests now cover terminal-only, invalid
fresh, fresh/mixed, disabled/non-elected and wrong-owner cases. Rust tests retain the actual raw
failing batch and verify that it is rejected until the producer supplies the owner witness.
No capture layout, decoder rule, filter value, history policy or publication scheduling changed.

## Validation and remaining cross-owner blocker

Evidence is in this worktree's `target/improve-delivery/` (red logs, raw lanes, commands, test logs,
config hashes, captures and JSON analysis). `cargo fmt --check`, `cargo check`, 125 focused DDGI
Rust tests, all 24 native Slang CPU tests, and default hidden/muted Release smoke pass. Generated
files did not change. Tests still include the pending-completed-staging guard and black-atlas
acceptance. Temporary raw-lane logging was removed.

**The exact e2 fixture is still blocked, not accepted.** After fixing the missing evidence it
reaches geometry 4, field 7, e2, but exits 101 at the 15-second deadline with
`phase=capturing-inflight-stale-active`; no `.rfirr` is written. This is a second App fixture
contract conflict previously masked by the owner failure:

1. `environment_lighting_test_scene.rs` arms `CapturingInflightStaleActive` while geometry 4 is
   building and the previously completed geometry 3 field is Active at **e0**.
2. `ProductionCaptureCheckpointSource::capture_readiness` in
   `src/app/core/environment_irradiance_capture.rs` requires that old Active/new Staging window.
3. `DdgiCaptureTarget::Epoch(2)` correctly rejects e0. When geometry 4 reaches e2, Staging has
   already published; the App's in-flight readiness predicate is then false.

Those App files are outside DDGI ownership. The controller/App owner must reconcile an explicit
epoch target with the stale-active fixture's observation window, with a readiness regression.
Do not weaken epoch matching, mislabel e0 as e2, delay normal publication, or disable the pending
publication guard to satisfy this command.

Diagnostic controls, **not substitutes for that acceptance**:

- The same in-flight scene with target `published` completes, saves the real stale Active geometry
  3/e0 capture, and passes the unchanged current-format analyzer including owner/lineage,
  nonnegative content, luminance P99 >= 0.10, and existing direct-light ROI gates.
- Static `portal` target e2 completes and passes current-format owner/lineage/content analysis.

Both controls and the Release smoke shut down with `failures=0`, no ERROR/panic/VUID messages.
The e2 timeout remains a failed check. No visible game was launched, and no App/Tracer or saved
GUI/camera files were changed.
