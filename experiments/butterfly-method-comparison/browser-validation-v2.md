# Final browser checks — 2026-09-06

Both output-only pages were loaded in the root task's Codex in-app browser. No Chrome or Feishu page was operated on.

## Butterfly comparison

- Opened `comparison-v2.html` visibly and marked it as a deliverable.
- All three actual sprite atlases loaded. Tested all five direction choices, gray/blue palettes, and common timeline scrubbing.
- At `t=0.400s`, all three sprite labels showed frame 3 and the sampled source time showed `0.400s`.
- Both actual GLBs loaded, each reporting one shared animation. Inspected the source-model views with the current 16px frame overlaid.
- Source next-frame controls advanced from frame 3 through frame 5 and wrapped to frame 1 at `t=0.000s`.
- Tested half-speed continuous source playback: source time followed fractional timeline time, rather than quantizing to the sprite sample. Restored sampled mode, normal speed and gray palette.
- Expanded the real motion-layer diagnostics. At the tested 45-degree frame, wing-only visibly differs from bob/pitch; full and no-abdomen-lag look identical, consistent with the measured 16px limitation rather than a missing asset.
- Inspected small-size samples, frame strips, the model views and diagnostic canvases. Overall pose changes and broad color regions improve on v1; tiny abdomen segments are not generally readable. See `blender-v2/QUALITY_REVIEW.md` for the retained art limitations.
- The saved production reference copy and the current production PNG have identical SHA-256: `c2d7c8169301cabcb93cc666f12f9ab3f36801eb36bbb42170a8a0f1502c9b82`.
- Independently read the completed Blender validation: 75 replay images equal, 75 clear boundaries, matching raw/native16 loop endpoints, one clip and six animation channels. This is export evidence, not a production-art approval.

## Procedural material comparison

- Refreshed the existing `terrain-material-system-prototype.html` tab and marked it as a deliverable.
- Confirmed the four fixed-grid comparisons, current base-color legend and moisture state, with separate intra-voxel sampling panels.
- Switched between macro and micro detail and exercised the explicit filtering footprint control. Labels and rendering updated.
- Visual review found the initial dense sampling overlay too dominant. Added an optional fine-grid display through the owning agent; verified the final default has only coarse voxel lines. Toggling the checkbox works; numerical verification by the owner confirms unchanged filled samples.
- Restored macro detail, default footprint and hidden fine grid, then showed the page's heading/default fixture.
- No game renderer or shader implementation is claimed. Actual camera-distance filtering, GPU cost and primary/DDGI/secondary agreement remain open design questions.

## File and scope checks

Both final inline scripts parsed successfully. All 33 local `href`/`src` references checked over HTTP returned success. The game repository remained clean on `main`, 22 commits ahead of its existing remote-tracking branch. This iteration changed only experiment outputs outside the repository; no push, production-asset replacement or shutdown occurred.
