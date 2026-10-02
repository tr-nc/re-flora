# Splash screen — throwaway HTML study

**Question:** How should a warm-white / pale-yellow checkerboard, top-down pixel growth and a minimal loading line feel together?

Visual review only. **Not wired into the game**, no real startup progress, no saved settings, no npm/build dependencies. Implement the approved direction separately when integrating into the game.

## Open

From the repository root:

```sh
python3 -m http.server 4173 --bind 127.0.0.1
```

Open <http://127.0.0.1:4173/experiments/splash-prototype/> or add `?clean=1` to hide the review tools. Opening `index.html` directly also works; the page uses the repository's existing Pixelify Sans font with a fallback.

## Current direction — revision 2

- Full-bleed checkerboard with **no central clearing or overlay**. Only the title has a compact, solid white rectangular backing.
- Tile colors **#fdf8e4 / #f9f2d7**: each original color moved halfway toward their common midpoint, approximately halving the contrast without changing the overall warmth.
- Smaller title, centered at **36.5% of viewport height**; subtitle removed.
- A simple **3 px loading line at 86% of viewport height**, horizontally centered. No visible percentage or loading text. Simulated 20-second loop, unrelated to game startup.
- Two distinct, parity-bound animations, both viewed from above:
  - Pale squares: a seed splits and **two broad cotyledons open together**.
  - Yellow squares: **four leaves unfurl successively around the center**.
- Both sets of cells animate throughout. They do not alternate between occupied/empty sets. A quarter-cycle offset adds rhythm, but their shapes and growth sequences are genuinely different, not just time-shifted copies.

Sprites use 32×32 pixel lattices and 24-frame growth cycles (including holds), enlarged with nearest-neighbor integer scaling. Reduced-motion preference starts the demo paused.

The original three-layout comparison is preserved on `prototype/splash-screen` at `0f4d043e`. This revision converges on the user's full-checkerboard direction; old `?variant=` links now open the current design.

## Review controls

- Sliders: growth speed and cell size (mobile caps cells at 112 px).
- **Space**: pause / resume animation and simulated progress.
- **H**: hide / restore the prototype tools.

Everything stays in memory except the shareable clean-preview URL. The dark review bar and top-left study label are **not part of the splash design**.

## Validation

Browser-checked on desktop and mobile: full-grid composition, two distinct animation sequences, smaller white-backed title, low progress line, slider/pause controls, clean-preview toggle, no horizontal overflow and no browser errors. No Rust/shader/game changes; no game build needed.
