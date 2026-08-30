# Production lighting-mode acceptance

`--lighting-mode-acceptance <artifact.rflma>` runs the R13 E2 production acceptance transaction.
It owns the deterministic foliage-shadow fixture, camera, animation time, frame sampling serial,
dither, path-reference bounce/ambient controls, and exit. It does not mutate GUI configuration or
defaults.

One release-hidden process waits for a converged DDGI field, latches the camera, visible-terrain
revision, DDGI field, lighting revisions, and extents, then records these stages after a settling
frame:

| Stage | Terrain | Raster flora |
| --- | --- | --- |
| A | DDGI | DDGI |
| B | path reference | DDGI |
| C | path reference | Legacy |
| D | DDGI | Legacy |

Each stage copies the real `compute_output_tex` (`R32_UINT` RGBE), `compute_depth_tex`
(`R32_SFLOAT`), and `gfx_output_tex` (`R8G8B8A8_UNORM`) after the production trace/raster work. No
diagnostic shader flag or counter participates in the proof. Logs describe the transaction but are
not evidence.

The committed `r13-e2-production-v1` calibration is fail closed. The analyzer requires exact
identity and depth/alpha masks across all stages; bit-exact non-target pairs A/D and B/C for terrain;
bit-exact non-target pairs A/B and C/D for raster; and at least 16 changed masked pixels (and a
changed ratio of at least `1e-6`) on both independent comparisons for each target toggle. Unknown
schemas/calibrations, missing or malformed raw layers, empty masks, non-finite depth, one-sided
effects, and identity drift fail.

Run the single-process acceptance through:

```bash
scripts/check_lighting_mode_acceptance.sh target/lighting-mode-acceptance/r13-e2.rflma
```

The runner refuses existing output paths, scans the app and worktree run log for failures, and then
invokes `scripts/analyze_lighting_mode_acceptance.py`. Set `REFLORA_CARGO` to a cargo-compatible
wrapper when validating against a local dependency checkout.
