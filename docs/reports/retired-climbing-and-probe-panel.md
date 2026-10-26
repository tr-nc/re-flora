# Retire climbing plants and the Environment Probes panel

Player-requested removal, 2026-10-27. Local only; no publication or release packaging.

## Scope and ownership

- Removed the **Climbing Plants** declaration, actions, all session-vine modules
  (growth, rod/contact mechanics, pruning/regrowth, fixtures/reviews), App tick
  and terrain revision/load observers, and the unused vine planting enum/branch.
  Vines were session-only, so no garden-save migration is required.
- Old `climbing_*` settings and the now-unused `cpu_stem_band_rendering`
  checkbox are retired before generated adjustables are constructed. Loading
  does not rewrite the user's file; explicit Save drops obsolete controls.
- Removed the **Environment Probes** App section, temporary draft/UI state,
  UI module and unused presentation strings. Production DDGI, probe rebuilds,
  normal lighting/shadows, native diagnostic visualization and CLI options remain.
  Status tests now assert typed runtime facts instead of retired panel strings.
- The CPU-authored stem benchmark is an independent client of the shared band/block
  renderer. Its block resource/API is now named `cpu_stem_block_resources` /
  `show_cpu_stem_blocks`, not after the deleted plant feature. Synthetic path
  benchmarks remain; they do not plant or simulate climbing plants.
- Ordinary grass/flowers/trees, material IDs (including existing Ivy material and
  static house decoration), and persistent species IDs remain unchanged.
- GUI Rust fields were regenerated with `cargo check`. No shaders, GPU ABI or
  asset files changed. Historical climbing research and the offline garden report
  remain archival evidence, not current instructions.

## Validation

Passed:
- `cargo fmt --check`, `cargo check`, `cargo build --release`.
- `cargo test retired_`: 40 passed, including legacy enabled/disabled settings,
  whole-section and stray-field retirement, read-without-write, Save/reload and
  generated-settings construction.
- `cargo test debug_panel`: 11 passed; `cargo test search_tests`: 10 passed;
  `cargo test section_order_tests`: 2 passed.
- `cargo test ddgi::`: 123 passed; `cargo test flora::`: 36 passed.
- Exhaustive generic settings save test and CPU-stem fixture parser test passed.
  Counts overlap between filtered runs; they are not summed as distinct coverage.
- Hidden/muted Release UI captures for both retired titles show **No matching
  controls**: `target/retired-climbing-panel.png` and
  `target/retired-probes-panel.png`, both inspected.
- Probe capture explicitly used `--environment-probe-visualization` and
  `--environment-probe-rebuild-spacing-voxels 16`. Logs confirm relocation
  of the initial 4,913 probes at spacing 32 and rebuilt 35,937 probes at spacing
  16; the screenshot shows diagnostic markers while the GUI section is absent.
  This establishes native resource/rebuild/render operation, not a new convergence
  or visual-lighting acceptance claim.
- Ordinary `--hidden --mute --auto-exit 0.5` smoke and
  `--tail-latest-log 200` inspection passed. Accepted runs contain no
  ERROR/panic/VUID and shut down with `failures=0`.

Native command outputs: `/tmp/retire-vines-ui.log`,
`/tmp/retire-probes-native.log`, `/tmp/retire-vines-smoke.log`.
Run logs remain under the worktree's `target/re-flora-logs`.
The full repository suite was not rerun; existing baseline failures are not
claimed fixed. No performance benchmark, manual gameplay approval or visible
game launch was performed.
