# Fixed-Camera Frame-Stability Benchmark

This benchmark measures visible frame-to-frame instability with a fixed camera. It captures a
continuous sequence from the real release renderer after a warmup period, converts the presented
pixels to luma, and records absolute luma deltas for every adjacent frame pair.

The standard run isolates terrain tracing by disabling clouds, flora, particles, god rays, and lens
flare. It keeps the normal Vulkan surface, swapchain, tracer, composition, and post-processing path
active in a hidden 1280x720 logical-pixel window. The report records the actual physical render
extent (for example, 2560x1440 on a 2x Retina display).

```bash
python3 scripts/denoiser_bench.py run --report target/denoiser-bench/baseline.toml
python3 scripts/denoiser_bench.py run --report target/denoiser-bench/candidate.toml
python3 scripts/denoiser_bench.py compare \
  target/denoiser-bench/baseline.toml \
  target/denoiser-bench/candidate.toml
```

Lower values are better for the temporal-delta metrics. `mean_abs_luma_delta_8bit` describes
overall flicker, while the p95/p99 and noticeable-pixel ratio expose sparse bright flashes that the
mean can hide. The noticeable threshold is an 8/255 luma change. Reports include every
per-transition metric so an isolated startup or scheduling spike remains visible.

`mean_frame_spatial_gradient_8bit` is the mean horizontal/vertical luma gradient after averaging all
captured frames. Higher values preserve more stable spatial detail, so a candidate cannot appear
better merely by blurring the output.

Use the same camera preset, extent, warmup, capture count, and isolation flags for comparisons.
Release-mode results from `scripts/denoiser_bench.py` are the authoritative benchmark; unit tests only
validate the metric math and CLI contract. Version 1 reports can still describe the retired
history/fresh-sample denoiser modes; version 2 reports describe the raw SH plus VSM terrain path.
