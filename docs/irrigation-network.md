# Irrigation network MVP

The irrigation MVP uses a rigid, grid-snapped pipe network. It intentionally does not simulate a flexible hose, pressure, or water moving inside pipe geometry.

## Controls

- Select **Pipe** in the item bar or press `V`.
- On an empty network, press the left mouse button on terrain to place the source connector, drag, and release to commit the first route.
- For later routes, begin near the source or an existing junction. The preview and committed route use deterministic X, Z, then Y orthogonal segments.
- Right-click or change tools while dragging to cancel the preview.
- Select **Spray** or press `X`, then click within eight voxels of any powered pipe segment. The sprinkler snaps to the nearest point on that segment.

The first source is a small visible connector. It is the stable interface for a future well/pump implementation; it is not a simulated water volume.

## Model and connectivity

`IrrigationNetwork` is the source of truth for:

- grid-snapped `IrrigationNode` source and junction positions;
- stable `PipeSegment` IDs and endpoint node IDs;
- the source node that defines the powered graph component;
- nearest powered mid-segment attachment queries;
- render data for committed routes and non-mutating previews.

A sprinkler stores its attached pipe segment ID. Placement is rejected when there is no source-connected segment nearby. Particle emission and terrain-moisture writes independently check that segment's current source connectivity, so losing the source or graph connection means no flow.

## Rendering

`IrrigationPipeRendererResources` builds one metallic rectangular prism per orthogonal segment and one larger blue-grey source connector. It uses the existing sprinkler opaque raster pipeline with a static instance and has a fixed 1,024-segment capacity. Capacity overflow fails explicitly. The pass is profiled as `graphics.irrigation_pipes`.

## Current limits

- Routes start at the source or an existing junction, not at arbitrary mid-segment branch points.
- Pipe deletion, valves, persistence, pressure loss, pumps/wells, and flexible hose physics are not implemented.
- The existing terrain-brush sprinkler removal remains available; pipe removal is deferred until topology-edit UX is designed.

## Validation

Pure tests cover orthogonal routing, preview non-mutation, source connectivity, source removal gating, powered mid-segment attachment, disconnected-segment rejection, and pipe mesh shape. End-to-end validation uses the standard hidden muted release run and log inspection.
