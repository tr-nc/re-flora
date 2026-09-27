# Cloud architecture step — scope v1

Work ID: ARCH. Worker: `/home/terence/code/re-flora-agent-cloud-decoupling`,
branch `agent/cloud-decoupling`, baseline `7310bd8265cc12223d90ecdb890e239d2f753fa1`.
This step does **not** retire clouds, alter saved configuration/capture formats, or
publish the worker branch. Integration and retirement remain subsequent work.

Design and reintroduction sites: [rendering effect seams](../rendering_effect_seams.md).

## Executed acceptance checks

Artifacts below are relative to the worker's `target/arch-validation/`.
All GPU launches used `flock --close /tmp/re-flora-summer-gpu.lock`; all were hidden
and muted. Run logs were located using `--latest-log`/`--tail-latest-log` in this worker.

| Command | Outcome / evidence |
| --- | --- |
| `cargo fmt --check` | Exit 0, `fmt-final.log` (baseline also passed, `baseline-fmt.log`). |
| `cargo check` | Exit 0, `check.log`; Slang artifacts regenerated. No tracked generated-file changes. Existing 12 warnings remain. |
| `cargo test` | Exit 0, `test-final.log`: game 1,165 passed / 4 ignored; collision benchmark 4 passed. Baseline: game 1,163 passed / 4 ignored, benchmark 4 passed (`baseline-test.log`). |
| `cargo run --release -- --hidden --mute --auto-exit 0.5` | Exit 0, `smoke-final.log`, `smoke-final-tail.log`. No warning/error/VUID/panic in the run log; shutdown `failures=0`. God rays still allocate three R32 textures at 1440x810; retained cloud shadow allocation remains 256x256. |
| `cargo run --release -- --hidden --mute --windowed --resize-lifecycle-test --auto-exit 6` | Exit 0, `resize-final.log`. Five resize requests, final publication 1280x720, frame/swapchain/tracer generation 5 agree; shutdown `failures=0`. Existing fruit-physics hitch warning observed, also present in baseline runs. |
| `cargo run --release -- --hidden --mute --glass-voxel-test-scene --auto-exit 1` | Exit 0, `glass-smoke.log`. Full Glass allocation at 1440x810; no errors/VUIDs/panics, shutdown `failures=0`. |
| `cargo run --release -- --hidden --mute --no-shadows --auto-exit 0.5` | Exit 0, `no-shadows-smoke.log`; no errors/VUIDs/panics, shutdown `failures=0`. |
| `git diff --check` | Exit 0. No `config/gui.toml` rewrite. |

Final ordinary smoke log:
`target/re-flora-logs/re-flora-20260926-204634.807-528731.log`.
Final resize log:
`target/re-flora-logs/re-flora-20260926-204641.634-529046.log`.
Glass log: `target/re-flora-logs/re-flora-20260926-203729.850-521091.log`.
No-shadows log: `target/re-flora-logs/re-flora-20260926-203834.048-521431.log`.

## Additional checks and baseline limitations

These are **not** reported as passes or silently attributed to this change:

- `cargo test -p re-flora-vkn --lib`: 45 passed, 1 failed (`vkn-test.log`). Both new
  reflection tests pass: canonical temporal camera ABI and unique semantic daylight
  source descriptors across terrain/flora/tree/soil. The failure is the existing
  `particle_vertex_shader_reflects_one_compact_mesh_input_before_instances`, referencing
  absent `shader/particles/particle_lod_textured.vert`. Reproduced with all ARCH changes
  stashed and the worker at the clean baseline, using
  `cargo test -p re-flora-vkn particle_vertex_shader_reflects_one_compact_mesh_input_before_instances`
  (`baseline-vkn-particle-test.log`, exit 101). The task stash was restored and removed;
  unrelated pre-existing stashes were not changed.
- `cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 15`: exit 101,
  `raster-tree-smoke.log`. After successfully validating the original tree-lighting path
  at frame 20, point-light insertion hits `DDGI staging publication lost its owner radiance
  tuple`. The saved baseline Release executable reproduces the same panic at the same
  phase (`baseline-raster-tree-smoke.log`, exit 101). This extended scenario cannot
  finish on baseline either. Logs:
  `target/re-flora-logs/re-flora-20260926-204638.160-528965.log` and
  `target/re-flora-logs/re-flora-20260926-204657.030-529156.log`.
- RFIRR comparison was attempted, not accepted as bit-exact rendering evidence. Both
  baseline and candidate captured the sealed fixture at 960x540, epoch zero, five
  planes, with finite data. Baseline repeat captures themselves vary. The ordinary
  baseline capture also fails the existing analyzer's combined-product check
  (`baseline-capture-analysis.json`); no capture protocol was changed here.
  With `--no-flora --no-particles --no-leaf-shadows`, each capture's diagnostic validity
  checks pass, but `--compare` still exits 1 for non-bit-exact lighting/shadow planes
  (`static-capture-comparison.json`). World/hit positions are bit-exact. Differences
  are sparse, but not assumed harmless: the candidate/static baseline maximum shadow
  delta is 0.8403; a static baseline repeat also differs (maximum 0.07614), while the
  ordinary baseline repeat reaches 0.8362. See `static-capture-numerical-comparison.json`
  and `capture-numerical-comparison.json`. These wall-clock runs still include independent
  tree/fruit shadow producers; the flags do not freeze all scene simulation. Exact image
  equivalence remains unverified, rather than assigning a fabricated tolerance.

Capture command (replace `<name>` with baseline/candidate artifact prefix):

```sh
<release-binary> --hidden --mute --windowed \
  --no-flora --no-particles --no-leaf-shadows \
  --environment-lighting-test-scene sealed \
  --environment-irradiance-capture target/arch-validation/<name>-static-sealed.rfirr \
  --auto-exit 8
python3 scripts/analyze_environment_irradiance_capture.py \
  target/arch-validation/candidate-static-sealed.rfirr \
  --compare target/arch-validation/baseline-static-sealed.rfirr
```

The normal and exposed-face receiver routes were inspected separately; both now have
explicit constructors and regression guards, including protection against applying a
voxel-face offset to an already-exposed surface. Grass retains its independent rest-leaf
receiver. No performance claim is made. Enabled-cloud visual behavior is not exercised:
the production hard-disable remains intact for this architecture-only step.
