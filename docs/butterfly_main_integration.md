# Main integration into Butterfly Blend

## Scope

User-authorized direction: `main` → `butterfly-blend`, not the reverse.
Main source: `2bf6385079dced7b84c461106b576a3408199eb3`.
Worker source before integration: `e85409dc620ba8f8d7aac6fcb00b5e5e31342e02`.
The user subsequently canceled shutdown; no shutdown was scheduled or performed.

## Saved settings

At the user's explicit request, `bec2c8ad` committed the previously uncommitted GUI settings:
raster trees and wind enabled, 16px butterfly tiles, 8 animation FPS, 0.8 wing transmission,
and preview enabled. These are Worker settings; main's settings were not changed by this commit.

The first combined test run exposed a test that hardcoded the previous transmission default 0.
The actual loader already uses the declarative default. Integration was aborted cleanly, then
`e85409dc` made that test derive the expected value/range from the authored config while still
asserting that every other saved setting remains unchanged. The operator documentation was
updated to match the explicitly committed defaults. No runtime migration policy was changed.
The merge was then performed again and all tests passed.

## Merge validation

No manual conflict resolution was needed. `cargo check` regenerated/checked bindings; the
tracked merged GUI bindings matched the generator, with no hand-edited generated output.
Compared all existing GUI parameters with `bec2c8ad`: every prior parameter was preserved.
The only added parameters are `ddgi_continuous_sampling` and `ddgi_aggregate_history`, both false.

- `cargo fmt --check`, `cargo check`, `cargo test`: 1045 main + 4 library tests passed, 2 ignored.
- 73 targeted Python and 17 Slang CPU tests passed.
- Hidden/muted release startup and canonical log inspection passed.
- Tree/resize smoke passed, including wind, stiffness, age/removal/replacement and real wood editing.
- Butterfly production fixture passed the 8/22/64px GPU/CPU hit-depth oracle, self-shadow switches,
  and transmission stages; GUI/camera bytes were preserved.
- Native terrain-edit regression passed: three brush edits, zero tree recompiles during edits.
  This single run's maximum logged edit frame was 28.83 ms with the Worker's enabled wind and
  butterfly preview. It is not a matched performance comparison against main's default settings.

Evidence: `target/main-integration/`. The pre-existing `target/butterfly-resume/` results were
preserved and restored; the fresh merged butterfly fixture lives in
`target/main-integration/butterfly-merged/`. Original log paths describe where it was captured.
The first failed test log is retained as `tests.log`; the passing rerun is `merged-tests.log`.
No visible app was launched, and no release or shutdown was performed.
