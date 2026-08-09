# Re: Flora Rendering Context

This glossary names the lighting-field concepts shared by Re: Flora's terrain and raster rendering
paths. Implementation plans and GPU resource details belong in `docs/`, not in this file.

## Environment Lighting

**DDGI Probe**:
A directionless spatial sample that represents surrounding diffuse irradiance and visibility over
the sphere.
_Avoid_: SH probe, point light

**DDGI Volume**:
A spatial field of DDGI probes with a defined transform, extent, and readiness state.
_Avoid_: probe cloud, ambient grid

**Irradiance Map**:
The directional diffuse-lighting function stored by a DDGI probe and queried with a surface normal.
_Avoid_: light color, visibility map

**Visibility Map**:
The directional first- and second-distance-moment field used to estimate whether a surface and a
DDGI probe are mutually visible.
_Avoid_: irradiance map, shadow map

**Global Sky Irradiance**:
The unoccluded directional diffuse irradiance of the authored sky, used outside a ready DDGI
volume.
_Avoid_: ambient constant, local fallback

## Rendering Roles

**Visible Terrain Publication**:
A coherent visible result for an authoritative voxel-terrain change; it exists only when every
affected terrain chunk is ready for observation.
_Avoid_: partial rebuild, mesh update

**Raster Consumer**:
A raster-rendered object that reads the environment-lighting field for shading without contributing
geometry to that field.
_Avoid_: DDGI probe, DDGI occluder

**DDGI Occluder**:
Geometry considered by DDGI visibility rays when updating the field.
_Avoid_: raster consumer

**Surface Normal**:
The per-surface direction used to query diffuse irradiance and apply DDGI surface-side weighting.
_Avoid_: probe normal
