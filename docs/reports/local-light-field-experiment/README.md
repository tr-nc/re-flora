# Shared local-light field experiment — evidence and capture provenance

Historical first-round evidence: 2026-11-03, implementation `74fdbf6b`.
[View the original HTML comparison](index.html). For the later fixed-32
receiver-factorization experiment and its rejection, see the
[follow-up HTML report](../local-light-field-32/index.html). The runtime candidate
from that follow-up was withdrawn; this bundle remains first-round evidence.

This bundle records an experimental result, not a claim of visual acceptance.
The checkbox remains off by default. Near-lamp warmth is reduced at the default
32-voxel grid; the 16-voxel comparison is a density experiment, not a new default.

## Image provenance

All four PNGs are **original app screenshots**, copied byte-for-byte from
`target/local-light-field/`; no crops, resizing, exposure correction, color
processing or AI image generation were applied. Native dimensions: 1600×1000,
PNG. Source: the local Re: Flora Release renderer, working changes based on
`e1389288`; `74fdbf6b` contains those changes. Camera: `garden-arrival`.
No external images were imported. Existing scene assets retain their existing
license/attribution requirements; their rights were not separately audited here.
There is no external source URL or generation-service prompt/model/seed.

| File | Role | Saved field option | Probe spacing | Capture delay / auto-exit |
| --- | --- | --- | --- | --- |
| [night-off.png](night-off.png) | Original/reference | false | 32 (default) | 10 / 12 seconds |
| [night-on.png](night-on.png) | Experimental comparison | true | 32 (default) | 10 / 12 seconds |
| [night-off-16.png](night-off-16.png) | Dense-grid reference | false | 16 | 20 / 23 seconds |
| [night-on-16.png](night-on-16.png) | Dense-grid experiment | true | 16 | 20 / 23 seconds |

For these captures only, `config/gui.toml`'s `time_of_day` value was changed from
0.47 to 0.95, and `grass_local_light_field` was set as listed. Both were restored
(0.47 / false) afterwards. No other defaults were changed.

Commands, with `FILE`, `DELAY`, and `EXIT` from the table:

```sh
target/release/re-flora --hidden --mute \
  --screenshot garden-arrival target/local-light-field/FILE \
  --screenshot-delay DELAY --auto-exit EXIT
# Dense-grid runs additionally use:
# --environment-probe-spacing-voxels 16
```

The dense experimental run also used `--perf`; the profiler overlay is not in
these screenshots. Simulation/wind/temporal lighting continue to evolve, so
separate runs are **not pixel-identical frozen-frame comparisons**. Changing
probe spacing also changes the original indirect field; compare A/B within each
pair, not the two B images as an isolated direct-only density measurement.

SHA-256:

```text
7d41aabee4b77a7d7a5ebf4003903571fb8ffdd6c5a24e43800e56ed512d73dc  night-off.png
169e84fa1818e90520edc94b2ce1543aaa0ce5889e7b30d05905d1c300e2a6ae  night-on.png
bd575294ee52ee955f03edfe325862cbee6a8213aead9f27be2f17313e19f309  night-off-16.png
9d4ed3ac86cadd9aa07d7c849a76fb77a2ef5099fcd35557678c5096b84d83e3  night-on-16.png
```

## Validation evidence

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed. Three existing Rust dead-code warnings remain.
- `cargo test`: 1838 app tests + 4 library tests + 1 display-authorization test passed; 19 ignored platform/expensive tests.
- Slang CPU tests: 61/66 passed, including the new `local_light_field_test` and existing `local_emitter_test`. Five pre-existing failures were independently reproduced by compiling the unmodified shader tree from `git archive e1389288`: `sprinkler_contact_contracts`, `sprinkler_radial_coverage`, `sprinkler_rotary`, `sprinkler_spray_continuity`, and `terrain_material_contract` (the last fails compilation). The all-tests runner stops at its first failure; the remaining tests were also run individually. No unrelated baseline was modified to obtain green results.
- Hidden/muted Release runs exercised A/B modes, the regular and denser grids, daytime/nighttime captures and the existing `point-light-changes` scenario.
- Opt-in GPU audit: `RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT=1`, with the checkbox enabled and `--perf` for detailed logs. At light/geometry/publication changes this compares every valid probe's 64 interior normal directions against the original direct evaluator at the same probe position. It does not validate interpolation at arbitrary grass positions or establish visual equivalence.
- Default lantern scene: 875,328 samples per audit, zero tolerance failures; observed maximum absolute RGB error ≈ 2.98e-8.
- Point lifecycle: 889,408 samples per audit, zero tolerance failures. It covered 8 selected lights/overflow, blocker insertion/removal, moving, recoloring/intensity changes, and removal. Maximum absolute error ≈ 1.83e-4 in the high-intensity multi-light phase, within `1e-4 + 1e-3 × reference_max_channel`; after removal all direct samples were zero. The existing point-light scenario also reached its final complete checkpoint with no stale direct contribution.
- The first lifecycle run's 60-second auto-exit expired before final DDGI convergence; repeating with 120 seconds completed. This was not hidden or reported as a successful first attempt.
- Normal runs do not execute the expensive reference audit. The final-receiver direct channel is fully rebuilt per enabled frame, without temporal accumulation; transport history and sun shading remain unchanged.

Local noncommitted logs/scripts: `target/local-light-field/`. Per-worktree authoritative run logs remain under `target/re-flora-logs/`.

## Performance evidence and limits

Two comparable Release runs, same default scene/camera and effective 8-light list,
32-voxel probe spacing, RTX 3060 Ti, **1600×1000 physical extent in these runs**:

```sh
target/release/re-flora --hidden --mute --auto-exit 16 --perf
```

Only the saved checkbox differed. Audit disabled. Last ~6 seconds:

| Metric | Original | Shared field |
| --- | ---: | ---: |
| Actual FPS from frame-counter advancement / wall time | 21.74 | 31.88 |
| Sampled GPU frame median | 36.12 ms | 22.78 ms |
| Grass lighting/pose preparation median | 19.74 ms | 6.09 ms |
| Direct-field generation median | — | 0.84 ms |
| Existing DDGI probe trace median | 1.80 ms | 1.70 ms |

GPU scope medians have only 4 / 6 samples in that tail; they are indicative, not
statistically rigorous acceptance. `[PERF][FRAME]` duration averages are biased
by conditional logging, so FPS is **not** derived from those averages. GPU frame
time is not the inverse of measured app FPS. Earlier 2880×1620 diagnosis numbers
must not be compared as if resolution were unchanged.

32-voxel direct atlas allocation: 29,811,200 bytes (~28.4 MiB) per used frame slot.
16-voxel allocation: 223,203,200 bytes (~212.9 MiB), plus existing DDGI allocations
also growing from ~150 to ~1122 MiB. The dense night screenshot run measured
~6.25 ms direct-field update; its camera/lighting differ from the daytime timing
pair, so it is not a clean overall-speed comparison.

The experiment supports moving repeated visibility work to probes. It does not
establish light-count-independent update cost, exact near-field shadows, or
approval to replace the original renderer. Local spatial refinement is the next
quality question; global density escalation or silently switching off shadows
is not a completed solution.
