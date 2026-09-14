# Local cicada rotation-audio repair

PetalSonic local commit: `3f5c68bf4e3c9fc939944b7b91434fbe960e7a72`,
in sibling checkout `../petalsonic`. No release or remote push was authorized.
The game's published dependency remains 0.9.1.

The native HRTF renderer hard-switched nearest-direction filters. Controlled
rotation-only tests reproduced a 0.20 sample jump with constant input, without
occlusion or source movement. The repair crossfades complete convolutions with
shared historical input, including FFT/FIR block-size transitions. See the
backend's `docs/native_hrtf_filter_continuity.md` for regression evidence.

For local testing, explicitly select the repaired checkout:

```sh
flock --nonblock --close /tmp/re-flora-summer-gpu.lock cargo run \
  --config 'patch.crates-io.petalsonic.path="../petalsonic/petalsonic"'
```

This command opens the visible game; run only when requested. Plain cargo run
without the override still selects the published, unfixed backend. A local
override rewrites Cargo.lock's PetalSonic source metadata; restore registry
metadata with ordinary cargo check before staging any dependency changes.
Do not commit an absolute local path or this temporary lockfile change.

Validation with the local override:

- cargo check; full game tests: 979 main + 4 library passed, 2 ignored.
- GPU-lock-protected release hidden/muted smoke passed, normal shutdown,
  failures=0, no ERROR/panic/VUID. Existing multiple-butterfly-atlas warning.
- Log: target/re-flora-logs/re-flora-20260914-231354.337-24687.log.
- Backend fmt/check/strict clippy, 183 unit + 6 integration + 2 doc tests passed;
  real-table rotation sweep passed; isolated release realtime gate passed
  after an initial 108 us / 107 us failure during concurrent compilation.

This fixes a proven discontinuity mechanism, not a claim that the user's exact
listening symptom is fully accepted. Subjective rotation listening remains
pending. The earlier cicada direct-occlusion bypass and user audio levels are
unchanged so the next comparison isolates this repair. No new GUI control,
asset, generated source, version bump or publication.
