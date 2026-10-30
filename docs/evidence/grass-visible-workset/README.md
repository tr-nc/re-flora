# Grass index/workset measurements

Historical evidence for two sequential optimizations. The visual report is [grass-visible-workset.html](../../reports/grass-visible-workset.html); this folder is the raw/reproduction bundle, not an implementation spec.

- `step1/`: e1870de6 versus the unchanged 32-chunk runtime preceding it. 2880×1620 window / 576×324 scene.
- `step2/`: e1870de6 versus the final shared GPU workset, detailed profiling with 1024 query capacity.
- `overall/`: the pre-step1 32-chunk runtime versus final, normal profiler capacity. Both final groups are 1600×1000 / 320×200 after the system-selected monitor changed. Each group's before/after viewport is identical; do not subtract across viewport/profiler groups.
- Each summary contains exact options, binary/config hashes, per-run statistics and sample counts; `*.samples.json.gz` retain raw CPU/GPU frames. FPS is frame-number / wall-clock, not inverse GPU duration.
- `native-summary.json`, `native/*.log.gz`: 17 final-binary native runs and both synchronization-validation runs. `silhouettes.json` contains 15 paired grass-mask comparisons; full-RGB equivalence is not asserted. Selected original captures and their provenance are under `images/`.
- `bounds.txt`: explicit Slang executable-backend test result. `validation.txt`: validation, limitations and discarded exploration.
- `manifest.json`: artifact and changed-source hashes. Baseline source is e1870de6; the workset sources are committed with this evidence. `target/grass-workset/` retains local executables and additional untracked captures/logs.

Reproduce with separate saved Release executables (never build or run other GPU tests during measurements):

```sh
node docs/evidence/world-frame-cost/run.mjs \
  --before /path/to/e1870de6/re-flora --after target/release/re-flora \
  --out target/grass-recheck --detailed
node docs/evidence/grass-visible-workset/validate.mjs \
  target/release/re-flora target/grass-native
node docs/evidence/grass-visible-workset/compare.mjs target/grass-native
slangc shader/tests/grass_visibility_envelope_test.slang \
  -std 2025 -I shader/slang -target executable -o /tmp/grass-bounds
/tmp/grass-bounds
node docs/evidence/grass-visible-workset/report.mjs
```

`compare.mjs` requires ImageMagick. Native runners use disposable copied configs and assert that the original config hash does not change. `RE_FLORA_GRASS_VISIBILITY=0` is a diagnostic unculled reference, not a saved gameplay setting. `RE_FLORA_GRASS_VISIBILITY_AUDIT=1` checks completed GPU results and is never enabled for timing; its name deliberately does not select a fixture world. `orbit-both-b` uses real painting/mowing/replanting and an advancing fixed simulation clock.

`sync.mjs` records the two-run synchronization check on this machine. It expects the saved `target/grass-workset/{step1,final}` binaries and the installed SDK layer path; adapt that path for another SDK. It enables `VK_LAYER_KHRONOS_validation` and `VK_LAYER_VALIDATE_SYNC=1`. No new renderer/GPU readback is enabled in ordinary performance runs. Buffer capacities and the lighting dispatch's maximum range remain source-sized; expensive lighting/pose work and indirect draws use the GPU-visible count.
