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

## Optimized CRC32

The existing transitive dependency `crc32fast 1.5.0` is now used directly instead
of the bit-at-a-time implementation. The standard polynomial, on-disk format,
checksums, both validation passes, retained file handle, and atomic fsync/rename
publication are unchanged. This removes custom checksum code rather than adding
an I/O cache, alternate format, or asynchronous state machine.

| Pass | Save (ms) | Load ready (ms) |
|---|---:|---:|
| 1 | 229.45 | 1100.12 |
| 2 | 225.07 | 1082.19 |
| 3 | 238.33 | 1072.69 |
| Median | 229.45 | 1082.19 |

Median save time fell 68.5% (3.18× faster); load-ready time fell 47.5% (1.91×).
Save checksum/write is now 47–50 ms; load preflight is about 31 ms and the second
read/checksum pass about 28 ms. Remaining publication/restoration is about
838–864 ms, with GPU transfers about 142–158 ms per operation. Avoid treating
this remaining rebuild cost as file I/O, or removing pre-mutation validation to
hide it. Further work should instrument publication stages before changing them.

`cargo check`, formatting, and 925 application tests passed (2 opt-in tests
ignored). The release benchmark completed all four passes, each with exact
terrain bytes, flora layout/growth/identities, and tree/leaf publication checks.
It successfully reads the fixture produced by the old checksum implementation.
No runtime errors were logged; the existing multiple-butterfly-atlas warning
remains. Local evidence: `target/terrain-bench/evidence/optimized.log`.
