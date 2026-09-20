# Butterfly Blender v2 — review-only modification plan

Status: implemented prototype, 2026-09-06. The plan below records preparation before modeling; the original-frame facts and first-party findings were incorporated before the five-pose design in KEY_POSES.md. Final measured results and artistic limitations are in validation.json and QUALITY_REVIEW.md. No production asset or repository code changes. Every v1 artifact is preserved.

## Confirmed v1 source, read live

Source: `../blender/butterfly-prototype.blend`, opened in Blender 4.5.13 LTS without saving.

- Exactly two animated objects: `Wing hinge R` and `Wing hinge L`.
- Head, thorax and abdomen have no animation. The abdomen is a fixed elongated object (scale approximately .065/.32/.065), contributing to the rigid-rod appearance.
- Wing angle is a pure 60-degree sine. All five source cameras use orthographic scale 2.6 and elevation 15 degrees.
- 33 mesh objects, 126 faces. The wing has several front/back layers of inset/stripe/highlight geometry, whose detail does not survive usefully at 16px.
- v1 geometry and camera create very thin front/rear silhouettes in several poses. Its 16px opacity range includes 8-pixel poses.

## Reuse boundary

Keep the proven export invariants and small, explicit workflow: saved `.blend` reopen, shared clip containing every animated part, 0..1-second source time, five camera rows, five common sampled times, no per-frame recentering, transparent raw renders, deterministic global palette mapping, source replay and true-loop endpoint checks.

The v1 `run.py` hard-coded two GLB channels. v2 must replace that assertion with checks for the actual intended body/wing/abdomen channels, not remove the shared-animation gate.

Retain the existing local Blender installation and renderer. Copy no v1 production files over themselves. All v2 generated files and scripts live only in this new `blender-v2/` directory. Root owns the final original/v1/v2 comparison page.

## Original intended change, subsequently checked against reference facts

1. Design for a readable 16px silhouette first: fuller wings/body proportions, few large color regions, no small wing spots or layered decorative stripes.
2. Compare the important open/closed/transition poses at 16px before polishing a continuous animation. Fixed camera elevation and framing may change globally to avoid front/rear edge-only poses, never separately by frame.
3. Use one fixed world reference with a child flight-pose transform. The world reference, camera target and canvas do not move; intentional body bob and pitch are preserved rather than being canceled by recentering.
4. Animate body response in coordination with the stroke: downstroke lift/pitch-up, later pitch-down, chest/abdomen follow-through with phase lag. This is stylized art direction, not a claim of biological simulation.
5. Replace the pure symmetric wing sine with a designed pose-to-pose rhythm; preserve one shared timeline across angles and ensure all body motions loop too.
6. Judge body motion by whether it survives into 16px pixels, not only whether the 3D preview technically contains a moving transform.

At preparation time amplitudes, final pose angles and camera elevation were intentionally not locked. The subsequent authored choices are in KEY_POSES.md; this plan is not a competing parameter source.

## First-party practice incorporated so far

- Thomas Vasseur's [Dead Cells production article](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-) advocates starting from a pixel model sheet, basic 3D appropriate to final size, and convincing minimal key poses/timing before adding interpolation. His discussion also acknowledges remaining pixel flicker. Application here: remove detail that is irrelevant at 16px; do not mistake continuous 3D motion for successful sprite motion.
- [Soyafire's own workflow description](https://www.reddit.com/r/PixelArt/comments/lo886t/i_made_a_tutorial_on_how_to_use_blender_to_create/) explicitly uses exaggerated model features for pixelization, a common animation rendered at changed camera angles, and pixel cleanup afterward. Application here: deliberate proportions and camera choices are legitimate; any future manual cleanup must be labeled separately from the model export.

These are workflow precedents, not authority for the specific butterfly body-motion amplitudes.

## v1 preservation fingerprints

```text
a68e7d7220ff1d8ca943447822f88efb4bf80895f013076c7fa2b92e08ed4db4  butterfly-prototype.blend
cd122b5392e3e6f8ecfe4e55b4389110d544d76e1380586c1b6f563cb4abb4da  butterfly-prototype.glb
c8d22f64c58db0696e29ea964a564748380111266005d6505c731905e7d02af2  create_export_blender.py
7cfb40c750d363a518c213dd5e641582171b322b101374ef30366e6879ad86dc  run.py
96e9c536292bd59eddab34263654b3ca26fb8318de68a9f6fbc383c6423f0aa3  atlas-16-blue-indexed.png
f604bf54c10876a18775558f713ae16f395a98256ab43fd6d99f031f8c84344f  atlas-16-gray-indexed.png
```
