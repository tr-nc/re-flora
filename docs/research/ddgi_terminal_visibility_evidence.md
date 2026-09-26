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

## Scope-v1 validation and second root cause

Evidence is in this worktree's `target/improve-delivery/` (red logs, raw lanes, commands, test logs,
config hashes, captures and JSON analysis). `cargo fmt --check`, `cargo check`, 125 focused DDGI
Rust tests, 30 environment-lighting tests, all 24 native Slang CPU tests, and default hidden/muted
Release smoke pass. The existing source-wiring guard in `src/environment_lighting.rs` was updated
for the completion interface and checks that both stores use it; it is not GPU execution evidence.
Generated files did not change. Tests still include the pending-completed-staging guard and
black-atlas acceptance. Temporary raw-lane logging was removed.

**At the end of scope v1 the exact e2 fixture remained blocked.** After fixing the missing evidence it
reached geometry 4, field 7, e2, but exited 101 at the 15-second deadline with
`phase=capturing-inflight-stale-active`; no `.rfirr` was written. This is a second App fixture
contract conflict previously masked by the owner failure:

1. `environment_lighting_test_scene.rs` arms `CapturingInflightStaleActive` while geometry 4 is
   building and the previously completed geometry 3 field is Active at **e0**.
2. `ProductionCaptureCheckpointSource::capture_readiness` in
   `src/app/core/environment_irradiance_capture.rs` requires that old Active/new Staging window.
3. `DdgiCaptureTarget::Epoch(2)` correctly rejects e0. When geometry 4 reaches e2, Staging has
   already published; the App's in-flight readiness predicate is then false.

Those App files were outside scope-v1 DDGI ownership. Scope v2 explicitly authorized their repair,
below. The retained timeout evidence remains red; it was not replaced by the later successful runs.
Neither repair weakens epoch matching, mislabels e0 as e2, delays normal publication, or disables
the pending-publication guard.

Diagnostic controls, **not substitutes for that acceptance**:

- The same in-flight scene with target `published` completes, saves the real stale Active geometry
  3/e0 capture, and passes the unchanged current-format analyzer including owner/lineage,
  nonnegative content, luminance P99 >= 0.10, and existing direct-light ROI gates.
- Static `portal` target e2 completes and passes current-format owner/lineage/content analysis.

Both scope-v1 controls and the Release smoke shut down with `failures=0`, no ERROR/panic/VUID
messages. No visible game was launched, and scope v1 changed no App/Tracer or saved GUI/camera files.

## Scope-v2 fixture readiness repair

The fixture now waits for the **exact requested published old Active field** before issuing its
close/reopen overlap for later-epoch or terminal capture targets. Existing Tracer accessors supply
the capture target and checkpoint; no generic App orchestration or renderer scheduling changed.

Two interface shapes were considered. Merely waiting for e2 before the original sequence is
insufficient: production intentionally publishes the first complete terrain candidate even when
newer terrain is queued. Waiting for the second builder would therefore replace that old e2 with
the first candidate's e0. Instead the fixture owns an `OverlappingEdits` observation: the exact old
field identity, immutable first-edit builder token, and latest queued revision. It reopens the same
skylight while that first builder is incomplete. Both capture-frame observations validate this
window; a different Active, builder, latest revision, or completed staging invalidates the frame.
This is observation of normal progressive publication, not a request to pause it. It does not
pretend the queued latest revision is already the builder's geometry.

`published` and default e0 retain their original `LatestTerrain` window, capturing geometry 3/e0
while geometry 4 rebuilds. Other scenes and runs without capture retain their previous sequence.
The new seam lives only in `environment_lighting_test_scene.rs` and
`environment_irradiance_capture.rs`; RFIRR, shader math, strict owner/lineage decoding and the
current-format analyzer are unchanged.

Evidence is separate under `target/improve-delivery/v2/`. Two baseline-readiness tests first failed
with the former any-published-field rule (`readiness-red.log`), then passed with exact epoch and
checkpoint gating. The production readiness seam and full-frame capture coordinator tests reject
e0/e3, e2 of new geometry, missing Active/staging, ready staging, wrong/replaced builder tokens and
changed pending revisions; recovery requires a fresh complete frame. Tests also cover fixture
phase exposure and the unchanged published/default path.

The original command ran twice, including the original output path and 15-second deadline; each
artifact was moved to its own `legacy-e2-run{1,2}/capture.rfirr` before the next run. Both exit 0 and
pass `analyze_current_environment_irradiance_capture.py` with the unchanged correctness,
nonnegative-RGB, luminance-P99 >= 0.10, exact identity and direct-light ROI gates. The analyzer
commands and process-bound console/canonical logs are retained in that directory.

| Fact | Both e2 runs |
| --- | --- |
| Old published Active | geometry 2, token 1, field 3, **epoch 2** |
| Exact history source | geometry 2, radiance 1, field 2, epoch 1 |
| In-flight first edit | geometry 3, token 2, `Rebuilding` |
| Newest queued terrain | revision 4 (the real reopen edit) |
| Staging progress at arm / record | 512 / 1024 of 4913 probes |
| Irradiance / visibility-history / sample owner masks | 2 / 2 / 2 |
| Complete epoch evidence | 4913 probes, 245440 visibility samples |
| Environment luminance P99 | 0.1129139441 |
| Direct-light sunlit ROI mean / shadowed ROI max | 0.1601848079 / 0 |
| Analyzer validation failures | none |

The two entire RFIRR files are byte-identical (SHA-256
`436f6a7ad3b10798f5a0c4d4b4152093ae96986e3f60e995ca313755f18b89f5`). Neither captures the newer
geometry after publication. New published/default runs also pass strict analysis, and both entire
files are byte-identical to the retained scope-v1 published control (SHA-256
`049b287fbad456d2404050ec0bb889fe53cd8798cde14ee39acd4c9dcfa5de6e`).

Affected validation passes: fmt/check, 66 App environment tests, 125 DDGI safety tests, six
capture-frame tests, and Release hidden/muted smoke. All five new native logs have shutdown
`failures=0` and no ERROR/panic/VUID diagnostics. Initial GUI/camera hashes still match; generated
files are unchanged. Unchanged native Slang and broad sustained-edit/light-toggle/indirect-response
proof below is reused rather than rerun. Controller review, integration, full-suite and final
aggregate response/native acceptance remain pending: this repair is implementation-ready, not
final acceptance.

The prevention is local: capture fixtures must establish the requested old field **before**
mutating it and retain the exact observed overlap identity, rather than assuming a future epoch
will occur in a window whose geometry is already being replaced. No production scheduler change
or generalized rendering transaction is required.

## Sustained-edit and response checks (scope-v1 evidence, reused in v2)

The two existing Python runners now accept `--gpu-lock-held` **only** for a caller already holding
`/tmp/re-flora-summer-gpu.lock` over the entire run. Default internal locking is unchanged. This
avoids nested acquisition under the delivery contract's `flock --close`; it changes no fixture,
analysis, threshold or deadline. Help documents the precondition and full invocation. Mocked
runner tests verify both ownership modes and config restoration on launch failure.

```sh
CARGO_BUILD_JOBS=4 flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  python3 scripts/check_ddgi_sustained_edits.py target/improve-delivery/sustained32 \
  --spacing 32 --binary target/release/re-flora --gpu-lock-held
# Same command with --vary-lights and a different output directory.
CARGO_BUILD_JOBS=4 flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  python3 scripts/check_ddgi_indirect_response.py target/improve-delivery/response32 \
  --spacing 32 --binary target/release/re-flora --gpu-lock-held
# Same command with --static-control and a different output directory.
```

All four Release runs pass their unchanged checks. Each sustained run executes 40 advancing edits,
with 22 useful publications during editing; the light variant executes ten toggles and catches up
to final geometry 42/radiance 11. First 10% indirect response during edits is 306 ms on / 311 ms
off; off reaches 90% at 517 ms, with a negligible final residual. Static control also passes.
These are bounded correctness/response observations, not a new performance or visual-quality claim.
Process-bound console and canonical logs have no ERROR/panic/VUID or nonzero shutdown failures;
initial GUI/camera hashes still match.

Ninety focused Python tests (runners, lock ownership and capture analyzer), targeted Ruff, and CLI
help/error checks pass. Global `pyright` is unavailable; no type-check pass is claimed.
