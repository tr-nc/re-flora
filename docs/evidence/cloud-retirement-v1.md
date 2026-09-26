# Cloud retirement — scope v1

Work ID **REMOVE**, task `cloud-retirement`. Worker:
`/home/terence/code/re-flora-agent-cloud-decoupling`, branch `agent/cloud-decoupling`.
Started from ARCH `9fb151633486951e032417ca5f1d53a5288af703`; integration baseline
`7310bd8265cc12223d90ecdb890e239d2f753fa1`. No other worktree edits, delegation,
visible launches, push, integration, or worktree deletion.

Implementation commits:
- `9cc485e2d472322a309848085081c92bc577b11c`: retire rendering/lifecycle, GUI and
  parameters; migrate RFIRR/config/CLI boundaries; add policy/reflection tests.
- `7ea44622d30a4f91f04e6fb48cb0f863105bd406`: fix historical capture error reporting discovered by
  lint, with a regression test. The following evidence-only commit records validation.

## Delivered boundaries and files

See [rendering effect seams](../rendering_effect_seams.md) for current ownership
and concrete reintroduction sites, and [RFIRR contract](../ddgi_transport_acceptance.md).

- Deleted `src/tracer/clouds.rs` and six cloud Slang implementation/helper files.
  Removed their four manifest entries, all allocation/registration/dispatch/clear/
  history/invalidation paths from `src/tracer/{mod,resources,pipeline_builder,
  extent_dependent_resources,direct_sun_shadow_runtime}.rs`.
- Retained canonical camera snapshots, reflection-based daylight providers, independent
  terrain/leaf receiver policies, and sky-owned composition. No unused atmospheric
  receiver field or neutral cloud placeholder remains. Continuous positions used by
  DDGI/local lights remain. Shared leaf-depth and soil-fallback policies have executable
  tests in `shader/tests/{direct_sun_receiver_policy,terrain_moisture_sun_policy}.slang`.
- Removed declarative controls from `config/gui.toml`, `CloudGuiParams`, upload/frame
  fields, and `RenderFlags.enable_clouds`. Exact legacy IDs are migrated only in
  `src/app/gui_config_loader.rs`; load/draw/sync/save/reload tests cover both saved
  enable states, moved IDs, unrelated settings, and idempotence. `src/cli.rs` explicitly
  consumes the deprecated no-op; active scripts/scenarios omit it without deleting cases.
- RFIRR v11 retains the 284-byte header and all five planes/evidence. The fifth plane is
  `(terrain, leaf, integrated_weight, combined)`, packed at the shader capture boundary.
  v1–v10 decode unchanged, including historical cloud metrics; v9/v10 fixtures remain.
  Rust/Python share a new v11 golden fixture. Current-only production analysis, its CI
  contract, lifecycle evidence, and the tree-branch source gate use the current schema.
  Point products remain strict; integrated products use the mathematically valid bounds,
  not widened tolerances or changed shading. Mixed-source fixtures test both domains.
- Updated current documentation, preserving historical measurements/commands and assets.
  Generated **only by `cargo check`**: `src/app/generated/gui_adjustables_gen.rs` and
  `src/auto-generated/gpu_structs.rs`. No unrelated GUI saved-state changes remain.

Full implementation file inventory: `target/remove-validation/retirement-files.log`.
Runtime reference audit: no `cloud` references in 275 production shader/tracer/config/
manifest files (`runtime-retirement-audit.log`). Remaining references are compatibility,
negative regression guards, historical evidence, or unrelated assets/vendor content.

## Executed validation

Artifacts below are in **`target/remove-validation/`**, unless noted. All app runs
were hidden/muted Release, serialized with `flock --close /tmp/re-flora-summer-gpu.lock`.
After individual runs, the same worker's `--latest-log`, `--tail-latest-log 200`, and
`scripts/check_latest_run_log.py` were used. Full console logs and tails are retained;
the app's ten-log rotation removed some original per-worktree logs during the 100-run
transport suite. `smoke-summary.json` distinguishes retained originals from archived
console evidence. Exit codes and console panics were inspected independently: the log
checker alone misses Rust panics written only to stderr.

| Command | Actual result / evidence |
| --- | --- |
| `cargo fmt --check` | Exit 0; `fmt-final.log`. |
| `cargo check` | Exit 0; `check-final.log`. 121 native shaders; existing 12 Rust warnings. |
| `cargo test` | Exit 0; **1,166 game + 4 benchmark tests passed, 4 ignored**; `test-final.log`. |
| `python3 scripts/run_slang_tests.py` | Exit 0; **22 CPU tests passed**, including actual receiver/depth-gate and exhaustive soil source/readiness/coverage policies; `slang-final.log`. |
| `cargo run --release -- --hidden --mute --auto-exit 0.5` | Exit 0; no run-log warnings/errors/VUIDs/panics; shutdown `failures=0`; `smoke-final.log`, `smoke-final-tail.log`, `smoke-final-diagnostics.log`. |
| `cargo run --release -- --hidden --mute --windowed --resize-lifecycle-test --auto-exit 8` | Exit 0; **five requests**, final 1280×720, frame/swapchain/tracer generation **5** agree; no errors/VUIDs/panics, shutdown `failures=0`; `resize.log`, `resize-tail.log`. Existing fruit-physics hitch warning, also observed on baseline. |
| `cargo run --release -- --hidden --mute --glass-voxel-test-scene --auto-exit 8` | Exit 0; active full-resolution voxel-glass path, **490,298 glass pixels**, coverage 42.035%, **nonfinite=0, exhaustion=0**; shutdown `failures=0`; `glass.log`, `glass-tail.log`. |
| `cargo run --release -- --hidden --mute --no-shadows --auto-exit 1` | Exit 0; no fatal diagnostics; shutdown `failures=0`; `no-shadows.log`. Existing fruit-physics hitch warning. |
| `scripts/check_ddgi_transport_acceptance.sh --dry-run` | Exit 0; full inventory preserved; `ddgi-dry-run.log`. |
| `cargo run --release -- --help` | Exit 0; advertises `--no-clouds` as “deprecated; clouds removed; omit this flag.”; `help-after.log`. Canonical/legacy parsed run-plan equivalence has a Rust test. |
| `git diff --check`; `git diff -- config/gui.toml` after runs | Passed / no uncommitted config rewrite; `gui-diff-final.log` is empty. |

Final smoke original log (retained):
`target/re-flora-logs/re-flora-20260926-214151.216-574647.log`.

### Fixed foliage source-separated capture

```sh
flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- \
  --hidden --mute \
  --foliage-shadow-bench target/remove-validation/foliage-e0.toml \
  --foliage-shadow-bench-warmup-frames 90 --foliage-shadow-bench-frames 2 \
  --environment-irradiance-capture target/remove-validation/foliage-e0.rfirr \
  --environment-irradiance-capture-target e0 --auto-exit 15
python3 scripts/analyze_current_environment_irradiance_capture.py \
  target/remove-validation/foliage-e0.rfirr \
  --max-terrain-shadow-receiver-voxel-transmittance-range 0.000001 \
  --max-leaf-shadow-receiver-voxel-transmittance-range 0.000001 \
  --max-combined-shadow-receiver-voxel-transmittance-range 0.000001
```

Both exited **0**. `foliage-e0-analysis.json`: v11, 1440×810, finite valid diagnostics,
**98,269** multi-pixel receiver voxels, terrain/leaf/combined per-voxel max range **0.0**.
**1,055,843 point** and **27,999 integrated** samples. `foliage-e0-sources.json` verifies
nontrivial sources: terrain attenuates 290,040 hit samples, leaf 247,698, combined 337,653;
leaf minimum ≈0.14. **1,663** integrated samples do not equal the product of component
means, confirming that the new diagnostic rule is exercised by a real frame.
This is an epoch-zero spatial-invariant capture, **not** completion of the moving
foliage benchmark, convergence acceptance, visual approval, or performance evidence.

## Extended checks and bounded baseline failures — not passes

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests`: **373 passed,
  one error** (374 total), `python-final.log`. Missing `bd2` camera snapshot reproduces
  at baseline with the same test (`baseline-bd2.log`). All changed capture/lifecycle/
  CI/script tests pass, including historical compatibility and current-only rejection.
- `python3 scripts/check_shader_manifest.py`: exit 1, existing missing module declaration
  in `shader/slang/butterfly_mesh_types.slang`; identical baseline failure in
  `baseline-shader-manifest.log`, candidate `shader-manifest-final.log`.
- `cargo test -p re-flora-vkn --lib`: **46 passed, one failed**; `vkn-final.log`.
  Camera ABI, daylight semantic descriptors, and cloud-artifact/sky-binding absence pass.
  Existing missing `shader/particles/particle_lod_textured.vert` failure was reproduced
  at baseline by ARCH (`target/arch-validation/baseline-vkn-particle-test.log`).
- `uvx ruff check <changed Python files>`: 43 pre-existing diagnostics, matching the
  baseline rule/message inventory (`ruff-final.log`, `baseline-ruff.log`). The introduced
  missing-version-variable error was fixed and tested before handoff; focused F821 check
  passes (`ruff-name-check.log`). `pyright` is unavailable (`pyright.log`).

### Full DDGI transport attempt

Executed the unmodified runner under the GPU lock (2400-second safety timeout; **it
finished without timing out**, exit **1**):

```sh
flock --close /tmp/re-flora-summer-gpu.lock scripts/check_ddgi_transport_acceptance.sh
```

`ddgi-transport.log` / `.exit`; artifacts:
`target/ddgi-transport-acceptance/20260926T132724Z-559764/`.
All **100 capture launches** ran: **8 artifacts**, **92 console panics** (91 owner-version
errors; one density assertion). All eight artifacts decode as v11 and pass finite/source-
shadow diagnostic validity (`transport-schema-summary.json`), but transport energy/ROI/
bit-exact gates fail. The runner reports **24 top-level failure keys**, including nested
correctness/runtime/lifecycle failures. **No transport acceptance is claimed.**

Bounded baseline runs used ARCH's saved Release binary (`baseline-binary.sha256`) with
baseline `config/gui.toml` temporarily restored in this worker and restored to the candidate
in `finally`. No baseline worktree edits/builds were required. Exact argv/outcomes:
`bounded-baseline-runs.json`; console/helper/analysis artifacts have `baseline-` prefixes.

- Fixed foliage e8 and sealed e1 reproduce **“DDGI visibility sample evidence owner version
  is inconsistent”**, exit 101, on baseline. Candidate e8 failed before producing a capture;
  the successful e0 test above does not hide that limitation.
- Baseline sealed e0 reproduces **584,004 nonzero samples** where the acceptance fixture
  requires exact zero, exactly matching the candidate count.
- Baseline donor e0 reproduces the missing ROI samples / `None` direct-sun metric failure.
- Baseline density-changes e0 reproduces **Terrain vs Density** at
  `environment_lighting_test_scene.rs:5696`, exit 101.
- This is bounded failure-class reproduction, not a second baseline 100-run matrix.
  Forward/reverse bit-exact acceptance remains unverified; ARCH also recorded baseline
  repeat variation. Do not assign a fabricated tolerance or declare those comparisons good.

## Handoff status

Implementation and mandatory core checks are complete; worker changes are locally committed.
Independent review, integration/publication, and release acceptance are not performed here.
The extended suite remains baseline-blocked as recorded above. No performance claim or visible
try-out. Retain `target/remove-validation/`, `target/arch-validation/`, and the transport run
directory until the controller's final acceptance; no validation process remains running.
