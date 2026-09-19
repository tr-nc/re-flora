# DDGI edit-history reuse investigation

Worktree: `/home/terence/code/re-flora-agent-ddgi-history-reuse`, starting commit
`6fb81dfa`. This is an in-progress measurement effort, **not a validated flicker fix**.

## Temporal capture foundation

The merged baseline passed locked hidden/muted X11 release startup. Evidence:
`target/ddgi-baseline-smoke.log`; canonical log ends in `failures=0` and successful exit.

The existing screenshot owner now supports `--screenshot-sequence <count> <interval-sec>`
with `--screenshot <preset> <path> --screenshot-delay <sec>`. It writes numbered PNGs
and logs actual render-elapsed capture times (not writer completion times). Unlike a
single screenshot, a sequence deliberately observes unready scene/lighting frames.
There is at most one readback writer in flight; slow writers skip time rather than
queue duplicate catch-up captures. This is sampled display RGB, not HDR and not a
claim of every-frame coverage. Capture overhead must be reported separately from
performance runs. The owner joins its final writer before destruction.

Validation: `cargo fmt --check`, `cargo check`, `cargo test` (1029 main tests passed,
2 ignored), 12 targeted screenshot tests. Locked hidden release command:

```
env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock \
  target/release/re-flora --hidden --mute \
  --screenshot player-default target/ddgi-sequence-smoke --screenshot-delay 0 \
  --screenshot-sequence 4 0.1 --auto-exit 1
```

Four fresh numbered images and no ERROR/panic/VUID; logs
`target/ddgi-sequence-{check,all-tests,smoke,smoke-tail}.log`. No shader, generated
files, GUI defaults, terrain, physics, or lighting algorithm changed in this step.
No visible game launched. Temporal symptom reproduction and history experiments
remain outstanding; these startup images are capture plumbing evidence only.
