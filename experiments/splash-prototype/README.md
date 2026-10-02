# Splash screen — throwaway HTML study

**Question:** Which warm-white / pale-yellow grid layout best supports top-down pixel sprouts and a very simple loading line?

Visual review only. **Not wired into the game**, no real startup progress, no saved settings, no npm/build dependencies. Keep this experiment out of production integration; the chosen direction should be implemented separately after review.

## Open

From the repository root:

```sh
python3 -m http.server 4173 --bind 127.0.0.1
```

Open <http://127.0.0.1:4173/experiments/splash-prototype/>. You can also open `index.html` directly; it uses the repository's existing Pixelify Sans font with a fallback.

- `?variant=A` — **满格花圃**: synchronous sprouts across a full-bleed tiled field, with a small quiet area around the title.
- `?variant=B` — **棋盘萌芽** (default): sprouts alternate checkerboard cells; a wide central clearing separates the two garden bands.
- `?variant=C` — **留白画框**: a tiled border around a large quiet center, with one larger sprout above the title.
- Add `&clean=1` for a clean preview without review controls.

The thin progress line is at **68% of viewport height**, horizontally centered. No visible percentage or loading copy. It simulates a repeating 20-second load, unrelated to the game.

The 32×32 sprites have a 24-frame growth cycle (including holds): seed → split seed → two cotyledons → four-leaf rosette, viewed from above. Rendering uses nearest-neighbor integer enlargement, not smooth scaling of vector illustrations. Reduced-motion preference starts the demo paused.

## Review controls

- Bottom arrows / keyboard **← →**: change layout; the URL follows.
- Sliders: growth speed and cell size (mobile caps cells at 112 px).
- **Space**: pause / resume both animation and simulated progress.
- **H**: hide / restore the prototype controls.

Everything stays in memory except the shareable layout/clean URL. The dark review bar and top-left study label are **not part of the splash design**.

## Review status

Awaiting visual selection; no game changes. Browser-checked at 1440×900 and 390×844: three layouts, keyboard switching, slider input without accidental layout switching, pause, clean-preview toggle, no horizontal overflow and no browser errors. No Rust/shader files changed, so no game build is needed for this demo.
