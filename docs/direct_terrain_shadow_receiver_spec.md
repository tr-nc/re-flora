# Direct Terrain-Shadow Receiver Fix Specification

Status: `superseded` (2026-08-26)

This historical specification selected a continuous direct-terrain receiver to address a different
DDGI seam reproduction. It is superseded by
[`voxel_shadow_subvoxel_diagnosis.md`](voxel_shadow_subvoxel_diagnosis.md): the newer product
invariant requires filtered terrain direct-sun transmittance to be constant within one marcher
voxel. The DDGI hybrid receiver remains unchanged; only the terrain VSM receiver is canonicalized,
while leaf and cloud receivers remain continuous.

Accepted reproduction: commit `890bf98364f7d638eeebb9246973c1fe7fdcbe95`

## Problem Statement

The accepted `patt-seam` reproduction shows narrow grid-like lighting bands inside a diagonal
sunbeam projected through a terrain-edited roof opening. The bands are visible across the interior
floor and front-wall region. They are spatial transitions inside and around the projected beam, not
only aliasing at its hard silhouette.

The reproduction is deterministic and agent-runnable in the real release renderer. It starts from
the user-preserved `patt` camera, creates the roof opening through the same
`apply_surface_terrain_removal` operation used by the terrain tool, waits for terrain publication
and DDGI convergence, captures a screenshot, and applies a metric that excludes the principal
projected-light edge before counting secondary bands.

The user manually accepted the visible reproduction associated with
`target/re-flora-logs/re-flora-20260805-022230.215-172110.log`. Commit `890bf983` pins the accepted
camera, opening geometry, authored lighting, hidden-release wrapper, metric, and metric tests.

## Accepted Reproduction

Run from the repository root:

```bash
scripts/check_patt_ddgi_seam_repro.sh \
    target/ddgi-seam-repro/final/patt-seam-fixed.png
```

Before the fix, the command must report `[PATT_DDGI_SEAM] verdict=RED` and intentionally exit with
status 1. The accepted baseline is robust across the previously captured 1024x699, 1920x1080, and
2880x1620 swapchain extents.

The pinned scene uses:

- camera snapshot `patt`;
- roof-opening centers `(0.58, 0.8828125, 1.10)` and `(0.52, 0.8828125, 1.20)`;
- terrain-removal radius `0.08`, 24 passes at each center, and 48 total strokes;
- `time_of_day=0.49`, `latitude=-0.07`, `season=0.29`, and `sun_luminance=1.65`; and
- DDGI receiver visibility bias `0.0009765625`.

These inputs are acceptance fixtures. Do not retune them to make the implementation pass.

## Root-Cause Evidence

Disabling DDGI consumer visibility did not remove the accepted bands in the historical isolation
run. That release-hidden capture remained RED with essentially the same band structure as the
production visibility capture. In the accepted analysis region, their mean pixel change was
approximately `0.35 / 255`, while the symptom contrast was approximately `78 / 255` at the same
extent. The temporary consumer-visibility CLI matrix used for that experiment has since been
removed; `unoccluded-irradiance` remains the supported visibility-free diagnostic.

The exact DDGI visibility debug capture is almost uniformly visible in the affected region, and the
DDGI dominant-probe regions are substantially larger than the narrow repeated bands. DDGI moment
visibility does show a smaller discrepancy at concave wall/floor seams, but it is a separate
artifact and is not required to produce the accepted symptom.

The current direct-light path quantizes the direct terrain-shadow receiver to one location per
voxel:

1. `tracer.slang` calls
   `directLighting(albedo, result.normal, result.center_position)`.
2. `directLighting` passes that voxel center to `terrainShadowReceiverPosition`.
3. `terrainShadowReceiverPosition` calls `terrainRayOriginAlongNormal` in
   `terrain_ray_origin.slang`.
4. `terrainRayOriginAlongNormal` first calls `terrainVoxelSurfacePositionAlongNormal`, converting
   the voxel center into a canonical face-center position, then applies the outward offset.
5. Every fragment that hits the same voxel face therefore samples the filtered VSM shadow from the
   same world-space receiver position.

A smooth diagonal VSM transition consequently becomes a sequence of voxel-sized constant steps.
That mechanism matches the scale and orientation of the accepted bands and remains active when
DDGI consumer visibility is disabled.

## Chosen Design

Move only the direct terrain-shadow receiver from the canonical voxel face center to the exact
surface hit.

The direct-light call must supply `result.position`, not `result.center_position`, for shadow
receiver placement. The receiver origin must be:

```text
result.position
    + normalize(result.normal) * max(0, terrain_ray_origin_offset_world)
```

Keep the existing `terrain_ray_origin_offset_world` value and outward direction. Introduce or use a
clearly named helper whose interface accepts an already-resolved surface position, normal, and
offset. Do not pass the exact hit to `terrainRayOriginAlongNormal` under a parameter named
`voxelCenter`; that interface promises canonicalization from a voxel center and would make the two
receiver semantics ambiguous.

The direct-shadow receiver seam must make the distinction explicit:

- canonical voxel center -> canonical face-center plus outward offset, for callers that require
  voxel-stable behavior; and
- exact surface hit -> the same outward offset, for direct terrain-shadow visibility.

Only the direct shadow transmittance becomes continuous across a voxel face. Existing flat-voxel
albedo, Surface Normal, cosine term, and environment irradiance remain unchanged.

This design retains the same number of VSM samples and does not add a ray, buffer, texture, cache,
or host-side setting.

## Implementation Scope

- Update the direct terrain-lighting call in `shader/slang/tracer.slang` to place the terrain-shadow
  receiver from `result.position`.
- Add or adapt a narrowly named exact-surface outward-offset helper in
  `shader/slang/terrain_ray_origin.slang` if needed to keep the two semantics unambiguous.
- Add focused shader-aware or source-contract coverage that proves the direct receiver consumes the
  exact hit while the canonical voxel-origin helper still canonicalizes voxel centers.
- Regenerate shader-derived Rust output only through `cargo check`, and include it only if the
  source change actually changes generated output.

## Explicitly Out of Scope

- Changing DDGI Probe placement, relocation, transport, Irradiance Map construction, Visibility Map
  construction, consumer visibility, receiver bias, hard visibility, convergence, or publication.
- Changing the canonical DDGI receiver position or introducing a terrain DDGI receiver cache.
- Changing `result.normal`, normal generation, normal smoothing, or the cosine lighting term.
- Changing terrain albedo, palette lookup, fertility tint, material variation, or edit-preview
  tint.
- Changing VSM resolution, filtering radius, moment encoding, sampler behavior, or shadow-map
  projection.
- Hiding the bands by increasing blur, adding noise or dithering, temporally filtering the result,
  or changing the accepted camera, opening, lighting, framing, or metric thresholds.
- Fixing the separate DDGI moment-visibility discrepancy at concave boundaries.
- Refactoring unrelated tracer, terrain-edit, or renderer code.

## Risks

- **Self-shadow acne:** exact hit precision near the caster surface may expose self-intersection.
  Preserve the existing outward terrain-ray-origin offset and do not solve acne with a global bias
  increase.
- **Voxel-edge instability:** adjacent faces have different positions and may have different
  normals. True face transitions may remain discontinuous even though within-face quantization is
  removed.
- **Camera-motion shimmer:** canonical per-voxel receivers are temporally stable. Exact receivers
  move continuously with the visible hit and may expose VSM sampling instability during motion.
- **Flat-voxel appearance:** direct shadow transmittance will vary continuously across a face. Keep
  albedo, normal, cosine lighting, and DDGI irradiance voxel-canonical so the change does not become
  a material-style redesign.
- **Thin-geometry and contact-shadow regressions:** the receiver change may alter bias behavior near
  thin roofs, openings, terrain edges, or external casters touching terrain.
- **Misleading metric success:** removing or weakening the beam would also make secondary-band
  detection GREEN. Acceptance therefore requires the principal projected-light edge and beam
  contrast to remain present.

## Tests and Acceptance

### Focused Tests

- Add a focused contract test for the exact-surface helper: its result equals the supplied surface
  position plus the normalized normal times the non-negative existing offset.
- Preserve coverage for the canonical voxel helper: it still derives a canonical face-center before
  applying the offset.
- Preserve the existing screenshot-analyzer tests that distinguish one hard projected edge from
  multiple internal spatial bands.

### Deterministic Visual Acceptance

1. Before implementation, run the accepted command and record its RED verdict and same-worktree
   release log.
2. Change only the direct terrain-shadow receiver.
3. Run the same command without changing the fixture or analyzer thresholds. It must become GREEN.
4. A GREEN verdict alone is insufficient. The analyzer output must still satisfy its beam-presence
   conditions: contrast at least `30.0`, primary gradient at least `0.4`, and
   `primary_edge_excluded=true`. GREEN must result from the internal-band criteria no longer being
   met, not from loss of the beam.
5. Inspect the capture to confirm that the elongated diagonal beam and its principal projected edge
   remain clearly framed while the internal voxel-grid bands are absent.
6. Repeat at no fewer than two actual swapchain extents previously known to reproduce the symptom.

### DDGI Isolation Acceptance

- Repeat the fixed capture with `--ddgi-debug-view unoccluded-irradiance`. The production Moment
  path and the unoccluded diagnostic must both be GREEN, retain the principal beam, and show the
  same direct-shadow improvement.
- Capture the exact-visibility, moment-visibility, visibility-error, exact-irradiance,
  irradiance-error, weight-sum, and dominant-probe DDGI debug views before and after the change.
  They must remain unchanged within the deterministic capture tolerance. The final shaded view is
  expected to change because it includes direct lighting.
- Do not accept a change that makes the production Moment path GREEN while the unoccluded
  diagnostic retains the bands, or vice versa.

### Regression Acceptance

- Inspect thin roofs and roof openings for self-shadow acne and new light leaks.
- Inspect convex and concave terrain edges at multiple sun angles for edge discontinuities.
- Inspect terrain contact shadows from external casters, including the existing fruit/apple case.
- Move and rotate the camera through the fixed scene and a normal world scene to check VSM shimmer
  and temporal instability.
- Confirm the direct VSM sample count and renderer resource set are unchanged. If performance is
  measured, compare warmed release runs at the same actual swapchain extent.

### Repository Validation

Run the full AGENTS validation ladder after the shader change:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
```

Inspect the newest log from the same worktree for shader compilation, Vulkan validation, renderer,
panic, non-finite lighting, and device-loss errors. Also run the focused release-hidden reproduction
and inspect its same-worktree log. Do not launch a visible game until hidden validation and the
metric acceptance are satisfied; request the normal manual confirmation afterward.

## Completion Criteria

The implementation is complete only when the accepted `890bf983` fixture is unchanged, the same
reproduction transitions from RED to GREEN because secondary bands are removed, the principal
diagonal beam remains, full and disabled DDGI visibility agree, DDGI debug outputs remain stable,
the listed shadow regressions have been checked, and the complete repository validation ladder is
green. The renderer/shader implementation must be committed separately from this specification.
