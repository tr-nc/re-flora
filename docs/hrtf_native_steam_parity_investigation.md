# Native / Steam Audio HRTF Parity Investigation

Date: 2026-06-06

## Local Steam Audio Source Checkout

Steam Audio source has been cloned locally for follow-up inspection:

```text
/home/terence/code/steam-audio
commit 480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac
```

Useful files:

- `/home/terence/code/steam-audio/core/src/core/sofa_hrtf_map.cpp`
- `/home/terence/code/steam-audio/core/src/core/binaural_effect.cpp`
- `/home/terence/code/steam-audio/core/src/core/hrtf_database.cpp`
- `/home/terence/code/steam-audio/core/src/core/ambisonics_panning_effect.cpp`
- `/home/terence/code/steam-audio/core/src/core/phonon.h`

Permalinks for the same revision:

- SOFA coordinate conversion: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/sofa_hrtf_map.cpp#L507-L510>
- SOFA nearest lookup: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/sofa_hrtf_map.cpp#L93-L98>
- Binaural nearest/bilinear switch: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/binaural_effect.cpp#L67-L79>
- Overlap-add HRTF convolution setup: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/binaural_effect.cpp#L125-L140>
- Ambisonics HRTF precompute path: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/hrtf_database.cpp#L488-L519>
- Steam virtual speaker set for Ambisonics decode: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/ambisonics_panning_effect.cpp#L28-L55>
- HRTF interpolation enum docs in public header: <https://github.com/ValveSoftware/steam-audio/blob/480dd64f513cc8a6437e7d5b9eb0d3f1d30c2fac/core/src/core/phonon.h#L1568-L1578>

## Performance Benchmark Snapshot

Release benchmark command used for subsystem comparisons:

```bash
cargo run --release --bin petalsonic_spatial_bench -- --sources 1,8,36,64,128 --warmup 12 --blocks 80
cargo run --release --bin petalsonic_spatial_bench -- --sources 256 --warmup 8 --blocks 50

# Pure per-source HRTF comparison only, with Ambisonics excluded:
cargo run --release --bin petalsonic_spatial_bench -- --pure-hrtf-only --sources 1,8,36,64,128 --warmup 12 --blocks 80
```

Conditions:

- block size: 1024 frames
- sample rate: 48 kHz
- block budget: 21.33 ms
- Steam and native HRTF paths both use NH172 (`assets/hrtf/hrtf_b_nh172.sofa` / `.petalhrtf`)
- direct occlusion/reflections were disabled; this measured direct gain + HRTF/Ambisonics DSP paths

Current pure per-source HRTF median end-to-end time per audio block after native overlap-add convolution:

| sources | Native direct + Native per-source HRTF | Native direct + Steam per-source HRTF |
|---:|---:|---:|
| 1 | 0.009 ms | 0.006 ms |
| 8 | 0.067 ms | 0.048 ms |
| 36 | 0.302 ms | 0.238 ms |
| 64 | 0.539 ms | 0.425 ms |
| 128 | 1.089 ms | 0.904 ms |
| 256 | 2.312 ms | 2.295 ms |

Earlier all-mode scalar-FIR snapshot before native overlap-add convolution:

| sources | native direct + native per-source HRTF | Steam direct + native HRTF | native direct + Steam per-source HRTF | native ambi + native HRTF | Steam ambi + native HRTF | native ambi + Steam HRTF | Steam ambi + Steam HRTF |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 0.169 ms | 0.173 ms | 0.006 ms | 1.492 ms | 1.484 ms | 0.093 ms | 0.103 ms |
| 8 | 1.311 ms | 1.343 ms | 0.049 ms | 1.473 ms | 1.500 ms | 0.105 ms | 0.149 ms |
| 36 | 5.881 ms | 5.980 ms | 0.245 ms | 1.520 ms | 1.656 ms | 0.149 ms | 0.324 ms |
| 64 | 10.471 ms | 10.628 ms | 0.433 ms | 1.564 ms | 1.801 ms | 0.201 ms | 0.499 ms |
| 128 | 21.088 ms | 21.306 ms | 0.962 ms | 1.709 ms | 2.330 ms | 0.314 ms | 0.924 ms |
| 256 | 42.454 ms | 43.016 ms | 2.312 ms | 1.949 ms | 3.127 ms | 0.505 ms | 1.816 ms |

## Current HRTF Quality Hypotheses

### 1. Steam custom SOFA path needs an explicit Petal-local to Steam/SOFA direction mapping

PetalSonic native HRTF convention is:

```text
x = right, y = up, z = front
```

Steam Audio's SOFA map converts a Steam direction vector with:

```cpp
return Vector3f(-v.z(), -v.x(), v.y());
```

Petal-local `z=front` is now passed to Steam custom SOFA HRTF as `z=-front` in the per-source `BinauralEffect` path for apples-to-apples custom-HRTF comparisons. A temporary impulse parity check showed front/back matching only after flipping the z component before the Steam HRTF lookup, and in-game listening with Ambisonics disabled now sounds effectively indistinguishable between Native and Steam per-source HRTF.

### 2. Native and Steam per-source HRTF currently both use nearest direction lookup

Native source path:

- nearest direction lookup: `../petalsonic/petalsonic/src/spatial/native_hrtf.rs`
- fixed-block frequency-domain overlap-add convolution: `NativeHrtfRenderer::render_source_with_metrics`
- scalar time-domain FIR remains as a fallback/reference for unusual block sizes and tests

Steam per-source HRTF also supports nearest/bilinear, and PetalSonic currently requests nearest on the Steam path. No direction interpolation is enabled in either per-source path. If both use the same NH172 dataset and the direction mapping is aligned, per-source native and Steam nearest-HRTF output should be very close aside from implementation details.

### 3. Native Ambisonics binaural decode is intentionally simpler than Steam Audio's

Native Ambisonics decode currently projects measured `.petalhrtf` directions with uniform spherical weighting. Steam Audio instead builds Ambisonics HRTFs using a minimum-phase HRIR step, interpolation at 24 virtual speaker directions, and frequency-domain filters. So Ambisonics decode quality differences are more likely to be real algorithm differences than a simple bug.

## Next Debug Steps

1. Keep `Use Ambisonics` disabled for pure per-source Native-vs-Steam HRTF comparisons.
2. Run the benchmark with `--pure-hrtf-only` when comparing per-source Native vs Steam HRTF performance.
3. If future movement tests expose stepping artifacts, compare nearest vs bilinear direction interpolation explicitly in both paths.
