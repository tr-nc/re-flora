# Splash screen — throwaway HTML study

**Question:** How does a dark-green checkerboard with slowly turning yellow and white pixel flowers feel as a minimal splash screen?

Visual review only. **Not wired into the game**, no real startup progress, no saved settings, no npm/build dependencies. Implement the approved direction separately when integrating into the game.

## Open

From the repository root:

```sh
python3 -m http.server 4173 --bind 127.0.0.1
```

Open <http://127.0.0.1:4173/experiments/splash-prototype/> or add `?clean=1` to hide the review tools. Opening `index.html` directly also works; the page uses the repository's existing Pixelify Sans font with a fallback.

## Current direction — revision 3

- Full-bleed checkerboard in two close dark greens: **#294f40 / #305746**. No central clearing or extra grid lines.
- Each darker square contains a **yellow five-petal flower**; each lighter square contains a **white seven-petal flower**. Both remain fully open and visible: no growth cycle, sprouting or disappearance.
- **Discrete rotation, not continuous rotation:** 24 pre-rasterized orientations per flower, **15° per step**, with no interpolation, CSS rotation or crossfade. At the default 0.75× speed, each orientation holds approximately **1.07 seconds**, taking **25.6 seconds per revolution**.
- Yellow flowers turn clockwise, white flowers counterclockwise, with staggered stepping beats. Their silhouettes and colors are distinct. Each sprite is rasterized on a 32×32 pixel lattice and enlarged with nearest-neighbor integer scaling.
- Retained smaller title at **36.5% of viewport height**, on a compact white rectangular backing. No subtitle.
- Retained **3 px loading line at 86% of viewport height**, with green track and pale-yellow fill. No visible percentage or loading text. Simulated 20-second loop, unrelated to game startup.
- Reduced-motion preference starts the demo paused.

Earlier review snapshots: original three-layout comparison on `prototype/splash-screen` (`0f4d043e`), cream full-grid growth study on `prototype/splash-screen-checkerboard` (`1bfb5807`). Old `?variant=` links open the current design.

## Review controls

- Sliders: rotation speed and cell size (mobile caps cells at 112 px).
- **Space**: pause / resume animation and simulated progress.
- **H**: hide / restore the prototype tools.

Everything stays in memory except the shareable clean-preview URL. The review bar and top-left study label are **not part of the splash design**.

## Validation

Browser-reviewed at 1440×900 and 390×844, without browser errors or horizontal overflow. Verified 24 distinct, nonempty frames per flower, unchanged orientation between steps, discrete advancement and exact full-turn looping. Pause holds animation time; mobile review tools do not obscure the loading line. No Rust/shader/game changes; no game build needed.
