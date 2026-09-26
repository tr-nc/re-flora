# Strict leaf coverage boundary (scope v3)

Base: `a0b318770d1473974884e0bfe07dee293fc4f230`.

## Reproduction and diagnosis

The unchanged leaf runner passed six times at 5120×2880 on the worker. These
are retained, not evidence of a fix. Checking every pose also exposed the inverse
symptom (GPU addition absent from the CPU plan). A short stream of actual
published poses around the controller's failing flight time reproduced its exact
`instance=1 pixel=11,26` omission. Minimization retains one original variant 18,
its published quaternion/position/size, the fixture camera, and 64px sampling.

Run the deterministic **red on the base implementation** regression:

```sh
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  CARGO_BUILD_JOBS=4 node scripts/validate-model-coverage-boundary.mjs
```

The renderer-only fixture freezes the captured input, not a favorable replacement
pose. It requires the explicit native leaf review; ordinary play and the original
live A/B runner never enable it. Native failure is `instance=0 pixel=11,26` after
removing the seven irrelevant instances. No numerical assertions are skipped.

Ranked hypotheses and results:

1. **CPU center ownership removes a required repair:** disproved for this case.
   CPU and GPU both miss the center. The actual compute mode 1/3 tile producer
   calls `sampleModelPixelGeometry`, not the CPU repair graph. Mode 0 evaluates
   the graph but the tile producer never reads that result.
2. **Different projected geometry at a conservative cell boundary:** confirmed.
   CPU and GPU clip coordinates for triangle 29 match, but division rounding
   changes bounds and normalized tile coordinates. CPU bounds.x is
   -0.3390507698059082; GPU is -0.3390507996082306 (one ULP).
   CPU projected triangle 29 starts at (11.2487960226,25.6114219607);
   GPU starts at (11.2488756180,25.6114234924). The independent conservative
   edge test, with the unchanged 0.0001px epsilon, gives minimum signed margins
   +0.000034825125 on CPU geometry and -0.000068537943 on GPU geometry.
   The cell is outside even the CPU triangle without the epsilon expansion.
   Thus a CPU reprojection is not the geometry on which the GPU decided coverage;
   this assertion cannot require bit-exact cell membership from those two inputs.
3. **Wrong frame/resources/readback:** disproved by fixed-pose replay, matching
   camera view-projection/inverse bytes, original ray/triangle identity, and the
   targeted producer clip-coordinate capture. It remains red with no pose changes.

Raw captures, commands, every failed probe/build/run, and all initial green runs
are retained under `target/improve-delivery/v3/`. `red-capture-unprobed/` contains
camera/instance/triangle/tile/CPU-plan bytes before shader probes;
`red-capture/` also contains the producer arithmetic probe. Temporary probes are
archived as patches under `probes/`, not shipped. The minimized native red is
`fixture-red/`; CPU and GPU arithmetic comparisons are `cpu-arithmetic.log`,
`producer-arithmetic.txt`, and `coverage-margins.txt`.

## Correction contract

A correct diagnostic must independently classify the **actual projected geometry**,
just as it already uses actual GPU center ownership and independently checks its
rays. Two interface shapes were considered: emulate driver division/reassociation
on the CPU, or read back projected vertices (not coverage decisions) through the
existing native-review adapter. The latter keeps one geometry producer and a
small, explicit diagnostic seam; no GPU-specific arithmetic emulation, tolerance
increase, or production CPU plan is appropriate.

The correction must retain original RGBA/depth preservation, independently verify
projected geometry identity and surface depth, and reject a real missing covered
cell with a negative control. Ordinary production sampling must remain unchanged.
Final acceptance belongs to the controller.
