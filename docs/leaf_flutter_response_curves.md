# Unified flutter response curves

Debug → Flora → Leaves → Wind Motion now contains sibling **Amplitude Response**
and **Frequency Response** menus. Each has one Scaling slider and the same
three-handle editor: drag blue endpoints for wind thresholds/output levels,
gold midpoint for curve bias. No duplicate start/full/bias sliders or explanatory
paragraphs. Hover reveals precise values without permanent display-only text.

`flutter_response_editor` is one shared module with two adapters binding directly
to declarative saved fields. Drawing, hit testing, coordinate conversion and
shape inversion have one implementation. Frequency edits cannot change amplitude
and vice versa. Their different units/ranges remain intentional: amplitude is
a target excursion envelope, not an exact measured displacement on every cycle;
frequency is the mechanical time scale, not a metronome.

## Preserve existing authored behavior

Old Strength becomes the amplitude curve's high endpoint divided by two; low is
zero. The existing `leaf_local_displacement_voxels` value is kept unchanged and
relabeled Amplitude Scaling (voxels). Wind start/full/bias retain their existing
IDs and values. In this checkout: Strength 1 → high 0.5, low 0, Scaling 1.43,
wind thresholds 0.5/1.5, bias 0. Frequency values are unchanged.

Old files migrate on load; explicitly stored new endpoints win, and obsolete
Strength is removed by normal Save. No new save hook or duplicate App state.
All non-migrated config text was compared with the user's just-saved file and
found identical; unrelated user edits remain unstaged.

The shader interpolates two excitation endpoints while retaining the existing
noise, phase, mechanical solver and geometric radius. Calm still supplies zero
excitation even with a positive low endpoint. Low > high supports a decreasing
amplitude curve. Render/optics gates use the maximum of both endpoints, avoiding
accidentally disabling a low-positive/high-zero curve. Existing review-only gain
override resets low to zero to preserve its old off/strength semantics.

There is no state/vertex/push-constant size change. The previously unused w
component of the amplitude-curve vector carries twice its normalized low value;
the high value uses the existing gain channel. Generated GUI bindings come from
cargo check. No changes to world wind, audio, butterfly distribution or grass.

## Evidence and boundaries

Tests cover real mouse drag + normal Save/load for both adapters, cross-curve
independence, exact migration of other settings, idempotent legacy-file migration,
visible control uniqueness, old-force equivalence, falling curves and calm.
The shared serializer rounds floats to eight decimals; the drag save assertion
uses a 1e-7 tolerance rather than requiring identical bits for fractions like 1/12.

Correctness runs are not subjective visual/performance acceptance. No visible
game is started automatically and no other running process is terminated.

Validation: fmt/check, 986 main + 4 library tests (2 ignored), 14 Slang tests
passed. Release hidden/muted GPU validation under the shared lock passed:
`target/re-flora-logs/re-flora-20260916-002339.876-38782.log`.
Existing frequency sweep, phase remap, sustained flutter and bounds passed;
normal exit with shutdown failures=0. No ERROR/panic/VUID. The diagnostic run
retains the existing atlas warning and physics hitch; not performance evidence.
Runtime used the existing local PetalSonic override, with no dependency release.
Generated GUI bindings were regenerated; shader/particle ABI sizes unchanged.
