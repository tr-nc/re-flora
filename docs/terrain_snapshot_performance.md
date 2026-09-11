# Terrain snapshot performance

2026-09-11. Release-mode production-path measurements on this workstation
(RTX 3060 Ti), after Kochia removal. The fixture contains eight terrain chunks,
134,217,728 raw voxel bytes, 694 flora instances including four authored plants,
and two trees. Old user saves are not used.

## Reproduction

Serialize runs with the normal GPU lock. The first benchmark pass is warmup;
passes 1–3 measure the same loaded fixture with warm filesystem caches. Save
measures the GUI save entry point including quiescence and atomic publication.
Load measures the GUI load entry point **through water publication readiness**.
Exact terrain, flora, and tree/leaf checks run after each operation, outside the
measured interval. This is not a cold-disk or complex-garden scalability claim.

```sh
RE_FLORA_GARDEN_SNAPSHOT_SMOKE=seed flock --close /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --auto-exit 0.5 \
  --terrain-save target/terrain-bench/fixture.rflterrain
RE_FLORA_TERRAIN_BENCH=target/terrain-bench/benchmark-save.rflterrain \
  flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- \
  --hidden --mute --auto-exit 0.5 \
  --terrain-load target/terrain-bench/fixture.rflterrain
```

`[TERRAIN_TIMING]` records save and load stages, and `[TERRAIN_BENCH]` records
end-to-end time and exact-content verification. The benchmark output must differ
from its input fixture. It is opt-in and does not run during normal unit tests.

## Baseline

| Pass | Save (ms) | Load ready (ms) |
|---|---:|---:|
| 1 | 720.87 | 2058.37 |
| 2 | 729.32 | 2080.84 |
| 3 | 737.12 | 2062.27 |
| Median | 729.32 | 2062.27 |

Saving spends about 540 ms in checksum/write, 150–165 ms in GPU readback, and
20–30 ms synchronizing the file. Loading spends about 520 ms in preflight,
520 ms in the second read/checksum pass, 140 ms in GPU upload, and 840 ms in
publication and vegetation restoration. Both read passes validate CRC32 using a
handwritten eight-iterations-per-byte loop. First optimize that shared primitive;
the exact-open-file/pre-mutation validation contract must remain intact.

Local evidence: `target/terrain-bench/evidence/baseline.log` and `fixture.log`.
