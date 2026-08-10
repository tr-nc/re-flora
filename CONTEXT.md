# Re: Flora Context

This glossary names the player-experience and rendering concepts that define Re: Flora. Product
plans, implementation details, and GPU resource decisions belong in `docs/`, not in this file.

## Player Experience

**Restorative Garden**:
A full-screen garden space where active, low-pressure care helps a player transition away from work
and recover attention without creating another obligation.
_Avoid_: productivity game, background widget

**Garden Moment**:
A bounded interaction in which a small player action produces an immediate, legible, and beautiful
response from the garden.
_Avoid_: feature showcase, technology demo

**Free-form Planting**:
Player-directed planting without visible grid slots or a prescribed layout; environmental rules may
still constrain what grows on a surface.
_Avoid_: grid-free world, unrestricted spawning

**Gentle Consequence**:
A recoverable difference in growth, appearance, yield, or ecology that makes care meaningful without
creating irreversible loss or absence pressure.
_Avoid_: no consequence, punishment

**Sustainable Garden**:
An established garden that remains healthy while the player is away and can renew its basic plants;
it may continue growing or become wilder without losing prior work.
_Avoid_: maintenance loop, idle decay

**Seed Circulation**:
The small harvest-and-reinvestment loop through which surplus produce gives access to new plants,
while basic planting remains self-sustaining.
_Avoid_: market economy, order economy

**Personal Garden Story**:
The history expressed by the player's placement, care, growth, and return rather than an authored
plot, quest line, or dialogue obligation.
_Avoid_: campaign, narrative progression

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
