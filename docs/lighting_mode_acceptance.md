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
construct or destructure them. The two-stage timing/render plan remains a typed consuming seam, but
does not mandate UFCS or dot-call syntax. `RasterLightingMode` stays in planning and in the capsule
consumed by the GPU uniform sink. For Tracer's longer-lived draw/dispatch state, only the capsule can
construct an opaque `ResolvedRasterLightingState`; Tracer stores that value and can ask only whether
DDGI is active, so it cannot recreate a Legacy/DDGI value from a primitive or import alias.

Three startup-state interfaces were compared. `Default` was rejected because it would make the
lighting choice an implicit capability available wherever the type is visible. `Option` was rejected
because absence creates a second primitive path (`None` formerly meant DDGI). The selected interface
is an owner-issued initial capability: the acceptance module exposes a `pub(super)` constructor only
to App core, App moves its DDGI state directly into `Tracer::new`, and subsequent frames replace it
only from a borrowed `ResolvedLightingFrameInputs`. The opaque state is neither `Copy` nor `Clone`,
and its only observation borrows it. This preserves startup DDGI without giving Tracer a mode
constructor or an absence fallback.

Three seams were compared for proving that the opaque state stays non-`Copy` and non-`Clone`.
Extending the Python source checker with alias-aware trait resolution was rejected because it is a
shallow, incomplete Rust parser. Encoding the property indirectly in a field type cannot honestly
prevent manual trait implementations, while Rust negative impls are unstable. The selected seam is
`static_assertions::assert_not_impl_any!` in the owner's tests: rustc owns alias resolution,
qualified paths, derives, manual implementations, and generic target identity. The source checker
only guards the presence of that exact compile-time assertion; it does not interpret trait impls.

Rust privacy is the primary seal. Three raster-state seams were compared. Denying selected alias
tokens was rejected as too shallow; counting source occurrences globally was rejected as brittle;
the selected seam is the owner-constructed opaque raster state described above. Separately, three
source-audit seams were compared. A repository-wide unique bare function name was rejected because
ordinary builder helpers legitimately share names such as `update_buffers`. Fixed paths plus
substring presence were rejected because they cannot prove the receiver or a direct parameter type.
The selected structural tripwire parses the canonical `Tracer` and `BufferUpdater` inherent
implementations and the current inline `GuiInput` sink.

Rust type checking and the capsule's private construction are the ownership guarantee.
`scripts/check_lighting_mode_acceptance_source_contract.py` lexes every `src/**/*.rs` file only as a
structure-drift tripwire: it checks private resolved fields, external construction attempts, direct
capsule signatures, the App-to-Tracer initial move, the non-optional opaque Tracer field and its
single current capsule-factory assignment, inline GPU getter shape, and that current production
source contains no second `gui_input` write. It also ensures the owner retains the rustc-backed
non-`Copy`/non-`Clone` assertion without attempting to reproduce Rust trait semantics.
It deliberately does not infer control flow, require a plan-call spelling, or claim to detect
arbitrary shadowing and aliasing; it is not a proof of whole-program dataflow. Pure Rust logic tests
cover live and fixed plan resolution. The
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
