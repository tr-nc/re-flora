# Sprinkler implementation captures

Historical game captures, not AI image generation or the report's SVG illustration.
The plant spray-impact/wet-weight experiment has since been removed for performance
cost; Tall sprinklers remain. These files document that earlier experiment, not
current plant behavior.

## Source / attribution

- Creator: Re: Flora and its asset contributors; captured from the local Release
  app at source commit `377f75cf` (2026-10-25), before the later fruit-pivot
  correction `1d5bee2a`.
- Source: [Re: Flora source at 377f75cf](https://github.com/tr-nc/re-flora/tree/377f75cf),
  `src/app/core/sprinkler_review.rs` fixtures `rotary-tall` and `rotary-water`.
  Source revision identifies the local capture even if the commit is not yet
  published. No external image service or separate image asset ID.
- AI image provider/model/prompt/seed/negative prompt: not applicable; these are
  framebuffer captures, not generated artwork. Game runtime timing is not locked
  to a fixed simulation frame.
- Relevant saved settings: sprinkler size 0.65, apple size 0.8. The fixture
  owns emitter layout, camera, and its other review overrides. No separate
  compositing, painting, cropping, color correction, or normalization.
- Repository artwork is under CC BY-NC-SA 4.0 (see the project's
  `LICENSE-ASSETS` and individual asset notices). Attribution: Re: Flora and
  its asset contributors. License:
  https://creativecommons.org/licenses/by-nc-sa/4.0/ .
  These captures do not grant new rights to underlying assets. Component-specific
  notices still apply; no separate commercial-use/third-party-rights audit was
  performed for this report.

## Files

| File | Role / original output name | Specification | SHA-256 |
| --- | --- | --- | --- |
| [tall-original.png](tall-original.png) | Original; `rotary-tall-final.png` | PNG, 2880×1620, 8-bit sRGB, 565969 bytes | `585c2c1c471de02a5f1dca2620322189bff61895ea7b1804a880ff0c43728d40` |
| [plants-original.png](plants-original.png) | Original; `rotary-water-final.png` | PNG, 2880×1620, 8-bit sRGB, 643113 bytes | `6e6b492383e33ea454ef75ba409072d55adcba3fc1f91738bfa4110164d86961` |

The HTML report embeds these same PNG bytes as base64 data URLs so that the
report remains viewable when copied alone. CSS changes displayed dimensions
only; there is no image-file derivative.

## Reproduction

From the source worktree after `cargo build --release`:

```sh
for mode in rotary-tall rotary-water; do
  RUST_LOG=info RE_FLORA_SPRINKLER_REVIEW=$mode cargo run --release -- \
    --flat-terrain --hidden --mute --auto-exit 8 \
    --screenshot garden-arrival "$mode-final.png" --screenshot-delay 5
done
```

The `garden-arrival` capture preset triggers the existing UI-hiding path;
the review fixture sets the camera. This is not an unmuted/visible interactive
try-out, and the native window, Vulkan surface and swapchain remain in use.
Commands reproduce the scene/capture procedure, not byte-identical frames:
the capture delay is wall-clock based and sprinkler phase can differ.

The static images show appearance and scene context, not proof of oscillation,
recovery, terrain correctness, or performance. Those claims require the tests,
run logs, measurements, and a live A/B review described in the report.
