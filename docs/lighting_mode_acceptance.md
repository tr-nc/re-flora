# Production lighting-mode acceptance

`--lighting-mode-acceptance <artifact.rflma>` runs the R13 E2 production acceptance transaction.
It owns the deterministic foliage-shadow fixture, camera, animation time, frame sampling serial,
dither, path-reference bounce/ambient controls, and exit. It does not mutate GUI configuration or
defaults.

## Frame ownership

The acceptance module owns the opaque resolved timing and lighting frame inputs. Three seams were
considered: a neutral frame-input module would expose a shallow seven-value container or import an
acceptance-only factory; Tracer ownership would reverse the dependency by making the renderer own
fixture timing and phase policy; acceptance ownership keeps the fixed bundle, resolution order,
identity, and private construction in one module. Callers can consume the resolved views but cannot
construct or destructure them. `RasterLightingMode` remains typed through Tracer and is lowered only
when the opaque resolved lighting capsule is consumed by `BufferUpdater` at the production GPU
uniform seam.

Rust privacy is the primary seal. Three source-audit seams were compared. A repository-wide unique
bare function name was rejected because ordinary builder helpers legitimately share names such as
`update_buffers`. Fixed paths plus substring presence were rejected because they cannot prove the
receiver, a direct parameter type, or sink ownership. The selected seam parses the canonical
`Tracer` and `BufferUpdater` inherent implementations. `Tracer` passes its opaque
`ResolvedLightingFrameInputs` directly through a module-and-type-qualified production call;
`BufferUpdater` consumes that capsule in one inline `GuiInput` construction, so the four lighting
uniform values are never an independently forgeable primitive bundle at that seam.

Rust type checking and the capsule's private construction are the ownership guarantee.
`scripts/check_lighting_mode_acceptance_source_contract.py` lexes every `src/**/*.rs` file only as a
structure-drift tripwire: it checks the canonical qualified plan references, capsule signatures,
their actual chained UFCS initializers, the direct Tracer raster-state assignment from the same
capsule, inline getter shape, and that current production source contains no second `gui_input`
write. It is not a proof of arbitrary Rust aliasing or whole-program dataflow. The
shader-validation workflow evaluates the ordered `pull_request` and `push` path rules and routes all
current E2 source owners, this checker, its tests, and this document through the same CPU contract
gate.

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
not evidence. Only the analyzed A-D raw GPU artifact proves that the production toggles have the
calibrated effects; neither the Rust type seam nor the source-drift tripwire substitutes for it.

The checked-in `r13-e2-production-v1` candidate calibration is fail closed. It is not production
calibrated until a fresh artifact from this revision passes on the target GPU lane. The analyzer requires exact
identity and depth/alpha masks across all stages; bit-exact non-target pairs A/D and B/C for terrain;
bit-exact non-target pairs A/B and C/D for raster; and at least 16 changed masked pixels (and a
changed ratio of at least `1e-6`) on both independent comparisons for each target toggle. Unknown
schemas/calibrations, missing or malformed raw layers, empty masks, non-finite depth, one-sided
effects, and identity drift fail. If production calibration changes either threshold, record the measured
masked populations and two-sided changed counts in the commit that updates this calibration ID.

Run the single-process acceptance through:

```bash
scripts/check_lighting_mode_acceptance.sh target/lighting-mode-acceptance/r13-e2.rflma
```

The runner refuses existing output paths, scans the app and worktree run log for failures, and then
invokes the repository-owned `scripts/analyze_lighting_mode_acceptance.py` with the preflighted
`REFLORA_PYTHON` interpreter (default `python3`). `REFLORA_ANALYZER` is intentionally ignored: a
caller-selected analyzer cannot produce authoritative acceptance. The runner validates the
analyzer JSON schema, calibration, and `GREEN` verdict before reporting its own production GREEN.

Runner unit tests may explicitly set both
`REFLORA_LIGHTING_MODE_ACCEPTANCE_TEST_ONLY=1` and
`REFLORA_LIGHTING_MODE_ACCEPTANCE_TEST_ANALYZER=<path>`. That mode validates the same JSON contract
but reports only `TEST_GREEN`; it can never report authoritative `GREEN`. Set `REFLORA_CARGO` to a
cargo-compatible wrapper when validating against a local dependency checkout.
