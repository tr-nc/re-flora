# Independent cicada mix controls

Debug Panel → Audio has two saved channel pairs:

- Tree cicadas: canopy habitats (kind 2), existing Dog Day clip and -20 dB event trim.
- Grass / plant cicadas: grass and authored plants (kinds 0/1), existing Linne clip
  and -24 dB event trim.

Each has its own enabled switch and logarithmic scale slider. Master remains
shared. They route through separate backend buses, so updates affect active as
well as future voices. Existing spawning authority, call count/cadence, assets,
direct-occlusion diagnostic and limiter experiment are unchanged.

The old audio_mix.cicadas stored value is a read-time migration fallback for both
new channels, including enabled state. An explicit new channel takes precedence.
Save writes tree_cicadas and ground_cicadas, not an additional hidden combined
gain. Controls bind SavedControls fields directly; there are no per-control save
hooks. Existing user config remains untouched until the user chooses Save.

Validation: fmt/check, full 982 main + 4 library tests (2 ignored), including
legacy migration, independent channel round-trip, habitat routing, both
occlusion policies and the exhaustive custom-setting save/reload test passed.
No generated source changes. Test/build validation uses the local PetalSonic
output-limiter candidate; the published dependency remains 0.9.2.

After the old visible game exited, GPU-lock-protected release hidden/muted
validation passed: target/re-flora-logs/re-flora-20260915-010001.955-60123.log,
normal shutdown, failures=0. No ERROR/panic/VUID; existing atlas warning only.
