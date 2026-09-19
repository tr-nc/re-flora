# Cicada rotation-audio repair

PetalSonic fix: `3f5c68bf4e3c9fc939944b7b91434fbe960e7a72`.
After the user reported the rotation crackle appeared fixed during listening,
publication was explicitly authorized on 2026-09-15. Version 0.9.2 is published
on crates.io; annotated tag v0.9.2 points to release commit
`a695916b881ee86b892ee645ac2742ef29a81f16`. Remote main and tag were verified.
The game now consumes registry 0.9.2, with its checksum in Cargo.lock.

The native HRTF renderer hard-switched nearest-direction filters. Controlled
rotation-only tests reproduced a 0.20 sample jump with constant input, without
occlusion or source movement. The repair crossfades complete convolutions with
shared historical input, including FFT/FIR block-size transitions. See the
backend's `docs/native_hrtf_filter_continuity.md` for regression evidence.

Normal testing now uses the published repair, without a local path override:

```sh
flock --nonblock --close /tmp/re-flora-summer-gpu.lock cargo run
```

This command opens the visible game; run only when requested. No local path or
Git dependency is required, and no temporary override was committed.

Initial validation with the local override:

- cargo check; full game tests: 979 main + 4 library passed, 2 ignored.
- GPU-lock-protected release hidden/muted smoke passed, normal shutdown,
  failures=0, no ERROR/panic/VUID. Existing multiple-butterfly-atlas warning.
- Log: target/re-flora-logs/re-flora-20260914-231354.337-24687.log.
- Backend fmt/check/strict clippy, 183 unit + 6 integration + 2 doc tests passed;
  real-table rotation sweep passed; isolated release realtime gate passed
  after an initial 108 us / 107 us failure during concurrent compilation.

The user's listening feedback was positive, but does not rule out every other
possible audio artifact. The earlier cicada direct-occlusion bypass and user
audio levels remain unchanged. No new GUI control, asset or generated source.

0.9.2 publication passed the complete tools/publish.sh --publish gate: formatting,
workspace check, strict clippy, 183 unit + 6 integration tests, 2 doc tests,
release realtime budget, release demo build, package dry run and registry upload.
The release realtime gate passed on the first run during this publication.
Publication log: /tmp/petalsonic-092-publish.log.

Registry integration passed cargo fmt --check, cargo check, cargo test
(979 main + 4 library, 2 ignored), and GPU-lock-protected release hidden/muted
startup with no override. Cargo downloaded 0.9.2 from crates.io.
Runtime log: target/re-flora-logs/re-flora-20260915-003336.945-42707.log,
normal exit, failures=0, no ERROR/panic/VUID; existing atlas warning only.
Unrelated user config/gui.toml edits remain unstaged and preserved.
