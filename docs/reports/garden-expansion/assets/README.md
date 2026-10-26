# Native screenshot bundle — provenance

All 25 PNG files here are **original runtime review captures**, produced by the re-flora Vulkan Release application in the garden-expansion development session on 2026-10-26. They are not concept art, source textures, or simulated test-result illustrations. Each file's role, original relative filename, associated feature commit, known review hook, dimensions, PNG format fields, byte size and SHA-256 are in [manifest.json](manifest.json).

## Processing and reproducibility

Copied byte-for-byte from the original worktree's target directory: `cp target/<filename>.png <report>/assets/<filename>.png`. No resizing, cropping, normalization, recoloring, compositing or retouching was performed. The webpage scales their display only; clicking opens the unchanged original. These copies are the bundle's originals, so no external source file is needed to read the report. Use `node ../verify.mjs` to verify them.

Known native fixtures use `env -u WAYLAND_DISPLAY`, `RUST_LOG=info`, `cargo run --release -- --hidden --mute` and `--screenshot garden-arrival <output.png>`, except the shop's `player-default` preset. Bee captures use a five-second delay and 16-second run. Accepted reproduction recipes are in [the local command record](../evidence/reproduce.md). The feature commits identify relevant source implementations, **not proven exact screenshot build hashes**. Exact per-image build revisions and unrecorded moment-to-moment camera/random/timing state are unavailable; no bit-identical recapture promise is made. Screenshots were taken across implementation stages; the shop image predates the Bee house toolbar entry.

`garden-bees-tree.png` is explicitly a diagnostic: leaves, flowers and fruit were temporarily hidden to expose the mount. The ordinary bee scene is `garden-bees.png`. Butterfly before/after files preserve the original fixed-pose brightness comparison. Sapling filenames identify staged ages; the superseded 0.25-age experiment is not included.

## Generation and rights

Source/creator: the re-flora development runtime and the procedural/authored assets it renders. No external image/audio generation service was used for these captures. Image-model provider, image-generation prompt, sampler, negative prompt and image-model seed are **not applicable**. Native fixture settings are identified per file; unspecified capture state is explicitly unavailable rather than invented. Source URLs/asset IDs for the screenshots themselves are not applicable.

The project's code license file is GPLv3 and asset license file is CC BY-NC-SA 4.0; these do not establish rights to every third-party input. Images containing the sky inherit the unresolved evidence around the original Sun.png purchase/license holder (the moon is its runtime recoloring). The report does not grant redistribution or commercial permission. No RHS photograph was imported. Audio is not bundled here; the supplied wind recordings have separate unresolved commercial/redistribution rights. This is a local development/review deliverable, not public release clearance.

## Files

| File | Role / relevant implementation | Dimensions | Bytes |
|---|---|---|---|
| [garden-lantern-night.png](garden-lantern-night.png) | original native visual review screenshot; 01d030f9 | 4096×2560 | 1269768 |
| [grass-soil-tight.png](grass-soil-tight.png) | original native visual review screenshot; d5a67dfd | 4096×2560 | 1197535 |
| [garden-plow.png](garden-plow.png) | original native visual review screenshot; b7575aca | 4096×2560 | 1294365 |
| [garden-shovel-held.png](garden-shovel-held.png) | original native visual review screenshot; 0fbe7ca1 | 4096×2560 | 1249015 |
| [garden-shovel-drop.png](garden-shovel-drop.png) | original native visual review screenshot; 0fbe7ca1 | 4096×2560 | 1254998 |
| [garden-moon.png](garden-moon.png) | original native visual review screenshot; 557b3ae1 | 4096×2560 | 1674084 |
| [garden-blue-day.png](garden-blue-day.png) | original native visual review screenshot; 7393ba0d | 4096×2560 | 1165608 |
| [garden-blue-night.png](garden-blue-night.png) | original native visual review screenshot; 7393ba0d | 4096×2560 | 1135537 |
| [garden-fireflies.png](garden-fireflies.png) | original native visual review screenshot; 62ff5b98 | 4096×2560 | 1004079 |
| [garden-fireflies-day.png](garden-fireflies-day.png) | original native visual review screenshot; 62ff5b98 | 4096×2560 | 1200903 |
| [garden-lantern-day.png](garden-lantern-day.png) | original native visual review screenshot; 01d030f9 | 4096×2560 | 1199860 |
| [garden-fence.png](garden-fence.png) | original native visual review screenshot; d132893f | 4096×2560 | 1243167 |
| [garden-sunflower.png](garden-sunflower.png) | original native visual review screenshot; 55adfdd5 | 4096×2560 | 1166972 |
| [garden-sapling-0.375.png](garden-sapling-0.375.png) | original native visual review screenshot; 6be8beb6 | 4096×2560 | 1109686 |
| [garden-sapling-0.5.png](garden-sapling-0.5.png) | original native visual review screenshot; 6be8beb6 | 4096×2560 | 1110623 |
| [garden-sapling-0.75.png](garden-sapling-0.75.png) | original native visual review screenshot; 6be8beb6 | 4096×2560 | 1129283 |
| [garden-sapling-1.png](garden-sapling-1.png) | original native visual review screenshot; 6be8beb6 | 4096×2560 | 1135402 |
| [garden-butterflies-before.png](garden-butterflies-before.png) | original native visual review screenshot; 197c4df0 | 4096×2560 | 1209236 |
| [garden-butterflies-after.png](garden-butterflies-after.png) | original native visual review screenshot; 197c4df0 | 4096×2560 | 1209260 |
| [garden-bees.png](garden-bees.png) | original native visual review screenshot; 6c5fe738 | 4096×2560 | 1278429 |
| [garden-bees-tree.png](garden-bees-tree.png) | native attachment diagnostic screenshot; 6c5fe738 | 4096×2560 | 1130181 |
| [garden-shop.png](garden-shop.png) | original native visual review screenshot; 18089039 | 4096×2560 | 1268275 |
| [garden-harvest-before.png](garden-harvest-before.png) | original native visual review screenshot; 18089039 | 4096×2560 | 1133903 |
| [garden-harvest.png](garden-harvest.png) | original native visual review screenshot; 18089039 | 4096×2560 | 1134538 |
| [garden-fallen-harvest.png](garden-fallen-harvest.png) | original native visual review screenshot; 18089039 | 4096×2560 | 1136449 |
