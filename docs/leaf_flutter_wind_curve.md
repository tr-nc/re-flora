# Leaf flutter wind response

Flora → Leaves → Wind Motion now contains a saved wind-response curve:

- Flutter Start Wind: excitation starts above this local sampled wind strength
  (default 0.05).
- Flutter Full Wind: reaches full response here (default 1); stronger wind keeps
  flutter active, it does not stop it.
- Flutter Curve Bias: default 0 is smootherstep; negative responds earlier,
  positive later. Preview axes are wind strength and normalized response.

The same curve is followed as wind diminishes; the existing mechanical state
settles continuously. These are not time delays. Equal thresholds select a step;
separate them for ease-in/ease-out. Calm always supplies zero excitation.
The GUI orders full >= start, and the shader defensively accepts reversed inputs.

Previously speed²/(0.09+speed²), multiplied by gain and capped at one, saturated
early (at speed 0.3 with gain 2). The new curve replaces that mapping. Strength
0–2 scales its envelope to 0–1 without moving the full-response threshold.
This deliberately makes weak-wind/default motion gentler; amplitude remains the
independent maximum local radius, not a guaranteed excursion each cycle.
Cadence remains irregular per-leaf forcing; this curve changes its amplitude,
not its frequency. It is artistic tuning, not a botanical threshold claim.

All three parameters use declarative saved settings and the existing automatic
round-trip tests, with no new save hooks. The preview reuses the existing curve
widget/math; both leaf and legacy motion shaders share wind_response_curve.
Compute push constants grow from 32 to 48 bytes; leaf state/vertex ABI is unchanged.
Generated GUI bindings must come from cargo check.

## Validation

- fmt/check; Rust tests: 982 main + 4 library passed, 2 ignored. Declarative save
  round-trip and actual GUI control labels are covered.
- 13 Slang CPU tests passed, including onset/full, monotonic bias curves, reversed
  and equal thresholds, calm, gain independence and sustained-motion regression.
- Locked release hidden/muted GPU validation log:
  `target/re-flora-logs/re-flora-20260915-013249.821-67014.log`.
  Actual push constants logged `[0.05, 1.0, 0.0, 0.0]`. Steady-wind excursion
  0.083294705 rad, calm residual 0.00000001 rad; held bounds, lifetime remap,
  rapid reversal and display-cadence independence passed. Normal exit, failures=0.
  Existing multiple-atlas warning and instrumented physics hitch 112.824 ms:
  this is not performance acceptance. Runtime used the local PetalSonic limiter
  override; no published dependency or audio changes belong to this commit.
- Subjective appearance remains for manual review; no visible game auto-launched.
- Ordinary locked hidden/muted release smoke also passed, failures=0, no
  ERROR/panic/VUID and only the existing atlas warning:
  `target/re-flora-logs/re-flora-20260915-013330.155-71318.log`.
