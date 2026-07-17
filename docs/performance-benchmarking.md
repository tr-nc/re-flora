# Performance benchmarking

`config/perf_scenarios.toml` is the source of truth for named release benchmark workloads, tracked metrics, warm-up, minimum sample counts, and median regression budgets. `scripts/perf_suite.py` runs scenarios and writes raw samples plus summaries to JSON.

Performance conclusions require release-mode app runs. Debug builds and unit tests are not performance evidence.

## One run

```bash
python scripts/perf_suite.py run render-steady \
  --label baseline \
  --output target/perf/baseline.json
```

The command builds `re-flora` in release mode unless `--binary` points to an existing release binary. Use `--features slang-validation` to benchmark the full native-Slang aggregate.

Available initial scenarios:

- `render-steady`: fixed camera and settled GPU frame scopes;
- `surface-rebuild`: deterministic tree replacement with detailed surface timing;
- `tree-replace`: deterministic end-to-end construction timing.

Each run creates `<name>.json` and `<name>.log`. The report includes commit/dirty state, host, selected GPU, binary, command, raw samples, median, p95, range, and matched construction workload where applicable. A run fails if required markers are absent, a configured metric has too few samples, or fatal/validation diagnostics appear.

## Compare reports

```bash
python scripts/perf_suite.py compare \
  --baseline target/perf/a1.json \
  --baseline target/perf/a2.json \
  --candidate target/perf/b1.json \
  --candidate target/perf/b2.json \
  --output target/perf/comparison.json
```

Samples from repeated reports are pooled per side. Construction reports must have identical active voxel/brick/workgroup signatures. The command exits nonzero when a candidate median exceeds a configured budget; `--allow-regression` reports without enforcing that gate.

## Order-reversed A/B runs

Build each revision into a distinct target directory so both binaries remain available:

```bash
CARGO_TARGET_DIR=target/perf-a cargo build --release
# switch revision/worktree
CARGO_TARGET_DIR=target/perf-b cargo build --release

python scripts/perf_suite.py run-ab render-steady \
  --baseline-binary target/perf-a/release/re-flora \
  --candidate-binary target/perf-b/release/re-flora \
  --order A,B,B,A \
  --output-dir target/perf/render-ab
```

`run-ab` executes the scenario in the requested order, writes one report per run, pools A and B samples, and writes `comparison.json`. The default `A,B,B,A` order reduces thermal and temporal bias.

## Adding a scenario or metric

Keep workload ownership in `config/perf_scenarios.toml`. Supported metric sources are:

- `gpu_scope`: `name=valueus` fields from `[PERF][GPU_FRAME_SCOPE]`;
- `gpu_job_scope`: `[PERF][GPU_JOB_SCOPE]` durations;
- `surface_pass`: named millisecond fields from `[PERF][SURFACE_BUILD_PASS_TIMING]`;
- `tree_bench`: named millisecond fields from `[PERF][TREE_BENCH]`.

All values are normalized to microseconds in reports. Add parser tests when introducing a new log shape. Do not silently compare construction runs with different workload signatures.

## Validation

```bash
python -m unittest discover -s scripts/tests -p 'test_*.py'
uvx ruff check scripts/perf_suite.py scripts/tests/test_perf_suite.py
pyright scripts/perf_suite.py scripts/tests/test_perf_suite.py
```
