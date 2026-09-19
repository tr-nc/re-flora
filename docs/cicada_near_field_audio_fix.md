# Near-field cicada overload candidate

Backend repair: PetalSonic commit `510fc8e`, in sibling checkout ../petalsonic.
No new crate version was published; registry 0.9.2 includes the previous rotation
repair but not this output limiter. The source commit is pushed to backend main.

Audit found final hard clipping after resampling/headroom. The configured master
17 × cicadas 128 mix can saturate a single static direct call near the listener.
No duplicated bus gain was found. Distance attenuation is bounded at unity.
The fix adds 5 ms stereo-linked lookahead peak limiting, preserving normal PCM
apart from delay and reducing gain smoothly only for overload. It is not an
oversampled true-peak guarantee. Very strong overload can cause mix ducking.

Actual WAV/HRTF probe: Dog Day hard-clipped samples 101507 → 0, Linne 8401 → 0.
Far-distance peaks remain unchanged. Backend 189+6+2 tests, static checks and
three isolated release realtime runs passed; an initial 108 us / 107 us gate
failure remains recorded in the backend report.

Game fmt/check and 979+4 tests passed using the local override. Hidden/muted
release retry passed with failures=0 and no ERROR/panic/VUID:
target/re-flora-logs/re-flora-20260915-004933.592-55823.log.
The first run hung in ALSA/PipeWire physical stream opening during shutdown;
its stack was captured and only that agent-owned test process was terminated.
That incident is not proven fixed or baseline-only. See
/tmp/output-limiter-shutdown-stacks.log.

The system sink read 120% (+4.75 dB), which can overload downstream of library
protection. Neither system volume nor user GUI values were changed.
Subjective listening acceptance remains pending.

When the user requests the next visible trial:

```sh
flock --nonblock --close /tmp/re-flora-summer-gpu.lock cargo run \
  --config 'patch.crates-io.petalsonic.path="../petalsonic/petalsonic"'
```

No permanent local dependency override or generated-source changes. Cargo.lock
registry metadata is restored after local validation; preserve user config edits.
