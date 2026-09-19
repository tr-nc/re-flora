# AI route A handoff

Status date: 2026-09-06  
Execution mode: built-in `image_gen` with two local reference images  
Continuation gate: current executor cannot confirm GPT-6; stopped after preserving the first raw result and prompt as requested.

## Files

- Raw generated PNG: `ai-route-a-candidate-v1.png`
- Built-in original: `/home/terence/.codex/generated_images/01a0729a-b09c-71c3-bc3e-3e1eeb0b6839/exec-b2e79e05-aec7-43da-b88c-a72795d93ee3.png`
- Reference 1: `/home/terence/code/re-flora/assets/texture/butterfly_16px/butterfly.png`
- Reference 2: `/home/terence/code/re-flora/assets/texture/butterfly_16px/Blue.png`

## Raw result facts

- Dimensions: 1254 x 1254
- PNG SHA-256: `2122dbcc9542b447ee058655aef49c5c769ecb86bc1d6657745e02dfc3d1b691`
- ImageMagick reports sRGB, three channels, fully opaque, 105,227 colors.
- The apparent checkerboard is baked into RGB, not transparency. Therefore v1 fails the transparent-background requirement.
- Initial visual inspection shows a complete 5 x 5 contact sheet and broadly stable blue/purple butterfly identity, but column-to-column wing progression is entangled with view changes; same-column phase synchronization is not reliable enough to claim.
- No generated output has been copied into the repository or connected to game code.
- No iteration, crop/playback HTML, or comparison integration was performed after the GPT-6 handoff gate arrived.

## Exact prompt

```text
Use case: stylized-concept
Asset type: review-only pixel-art game spritesheet candidate for Re: Flora
Primary request: generate one clean 5 columns x 5 rows butterfly spritesheet, using the two provided repository images only as structural, animation, and palette references.
Input images: Image 1 is the existing 80x80 grayscale runtime atlas and defines the 5x5 layout, row/view order, compact butterfly silhouette, and hard-edged pixel language. Image 2 is the existing 80x16 blue five-frame strip and defines the cyan/blue/purple/black/white palette character and the five-step wingbeat rhythm.
Subject: exactly one small blue butterfly in each of 25 cells; the same individual design in every cell, with a compact black body, cyan-blue wings, restrained purple accent, white highlight, and black outline.
Composition/framing: a perfectly regular square 5x5 matrix, equal-size cells, no header, no labels, no separators, no margins between cells. Row order from top to bottom must be 0 degrees front, 45 degrees front-quarter, 90 degrees side, 135 degrees rear-quarter, 180 degrees rear. Column order from left to right is one five-pose wingbeat cycle. Every sprite must share the exact same body-center anchor, apparent scale, and bounding-box padding.
Animation invariant: column N must show exactly the same wing-hinge angle and same instant of the wingbeat in all five rows. Use five clearly distinct poses that form a continuous loop from column 5 back to column 1. Do not generate five unrelated poses per row.
Style/medium: deliberately low-resolution crisp pixel art, chunky nearest-neighbor pixels, strongly readable at 16x16 after reduction, matching the references' handmade game-sprite character.
Scene/backdrop: genuinely transparent background everywhere outside the butterflies.
Color palette: only transparent plus black, cyan, medium blue, restrained purple, and white; flat colors only.
Constraints: exactly 25 butterflies; fixed 5x5 grid; fixed center anchor; one consistent anatomy and markings; strict shared phase by column; transparent background with preserved alpha; no text; no numbers; no guides; no grid lines; no shadows; no ground; no effects; no extra objects; no watermark.
Avoid: anti-aliasing, blur, gradients, painterly edges, soft glow, inconsistent body length, changing wing markings, changing size, cropped wings, diagonal sheet perspective, contact sheets with captions, duplicated or missing cells.
```

## Recommended next safe action for the GPT-6 executor

Inspect `ai-route-a-candidate-v1.png` with `view_image`, record per-cell failures, then make exactly one targeted built-in `image_gen` iteration. The iteration should focus only on real alpha transparency and column-synchronized phase/anchor consistency. Preserve v1 unchanged. After selecting a final candidate, build crop/playback HTML from that actual PNG and emit a compact machine-readable manifest for the combined A/B comparison.
