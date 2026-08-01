module ddgi_query;

import ddgi_octahedral;
import ddgi_types;
import tracer_types;

static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_IRRADIANCE_STORED_SIDE = 10u;
static const uint DDGI_VISIBILITY_INTERIOR_SIDE = 16u;
static const uint DDGI_VISIBILITY_STORED_SIDE = 18u;
static const float DDGI_QUERY_WEIGHT_EPSILON = 1.0e-6;

[[vk::binding(23, 0)]]
Sampler2D ddgi_global_sky_irradiance;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(27, 0)]]
Sampler2D ddgi_irradiance_atlas;

[[vk::binding(28, 0)]]
Sampler2D ddgi_visibility_atlas;

public DdgiProbeMetadata getDdgiProbeMetadata(uint probeIndex)
{
    return ddgi_probe_metadata.data[probeIndex];
}

public struct DdgiQueryResult
{
    public float3 irradiance;
    public float accumulated_weight;
    public float accumulated_base_weight;
    public float mean_visibility;
    public uint contributing_probe_count;
    public uint dominant_probe_index;
    public float dominant_probe_weight;
};

public struct DdgiProbeContribution
{
    public bool trustworthy;
    public uint probe_index;
    public float3 irradiance;
    public float3 actual_position;
    public float3 nominal_position;
    public float base_weight;
    public float moment_visibility;
};

float2 ddgiAtlasUv(uint probeIndex, float3 direction, uint tileColumns,
                   uint interiorSide, uint storedSide, Sampler2D atlas)
{
    uint width;
    uint height;
    atlas.GetDimensions(width, height);
    float2 octUv = octEncode(direction) * 0.5 + 0.5;
    float2 texel = float2(ddgiTileOrigin(probeIndex, tileColumns,
                                        storedSide) + uint2(1u)) +
                   octUv * float(interiorSide);
    return texel / float2(width, height);
}

public float3 sampleDdgiGlobalSky(float3 normal)
{
    uint width;
    uint height;
    ddgi_global_sky_irradiance.GetDimensions(width, height);
    float2 octUv = octEncode(normal) * 0.5 + 0.5;
    float2 texel = float2(1.0) +
                   octUv * float(DDGI_IRRADIANCE_INTERIOR_SIDE);
    return max(float3(0.0),
               ddgi_global_sky_irradiance
                   .SampleLevel(texel / float2(width, height), 0.0)
                   .xyz);
}

public float3 sampleDdgiProbeIrradiance(uint probeIndex, float3 normal,
                                       U_ShadingInfo lighting)
{
    float2 uv = ddgiAtlasUv(
        probeIndex, normal, lighting.ddgi_irradiance_tile_columns,
        DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE,
        ddgi_irradiance_atlas);
    return max(float3(0.0),
               ddgi_irradiance_atlas.SampleLevel(uv, 0.0).xyz);
}

public float2 sampleDdgiDistanceMoments(uint probeIndex, float3 direction,
                                       U_ShadingInfo lighting)
{
    float2 uv = ddgiAtlasUv(
        probeIndex, direction, lighting.ddgi_visibility_tile_columns,
        DDGI_VISIBILITY_INTERIOR_SIDE, DDGI_VISIBILITY_STORED_SIDE,
        ddgi_visibility_atlas);
    return ddgi_visibility_atlas.SampleLevel(uv, 0.0).xy;
}

float ddgiChebyshevVisibility(float2 moments, float distanceToSurface)
{
    if (distanceToSurface <= moments.x) return 1.0;
    float variance = max(0.0, moments.y - moments.x * moments.x);
    float distancePastMean = distanceToSurface - moments.x;
    float probability = variance /
        max(variance + distancePastMean * distancePastMean, 1.0e-8);
    // Paper-style light-bleed reduction without a positive visibility floor.
    return probability * probability * probability;
}

float ddgiRelocationAwarePositionWeight(float3 fragmentInCell,
                                        float3 corner,
                                        float3 nominalPosition,
                                        float3 actualPosition,
                                        float3 spacingWorld)
{
    // Rohacek 2022 equations 1-4. probeOffsetNorm uses the paper's
    // actual-to-nominal convention; clamping enforces both weight invariants.
    float3 probeOffsetNorm =
        (nominalPosition - actualPosition) / spacingWorld;
    float3 offset = (corner - probeOffsetNorm) /
                    max(float3(1.0e-5), float3(1.0) - probeOffsetNorm);
    float3 axial = saturate(lerp(float3(1.0) - fragmentInCell,
                                fragmentInCell, offset));
    return axial.x * axial.y * axial.z;
}

float3 ddgiNormalizedSurfaceNormal(float3 surfaceNormal)
{
    float normalLengthSquared = dot(surfaceNormal, surfaceNormal);
    return normalLengthSquared > 1.0e-8
        ? surfaceNormal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
}

public bool ddgiQueryUsesGlobalSky(U_ShadingInfo lighting,
                                   float3 worldPosition)
{
    uint3 dimensions = lighting.environment_probe_grid_dimensions;
    float3 gridScale = lighting.environment_probe_world_to_grid_scale;
    if (lighting.ddgi_ready == 0u ||
        any(dimensions < uint3(2u)) || any(gridScale <= float3(0.0)))
    {
        return true;
    }
    float3 gridPosition = worldPosition * gridScale;
    return any(gridPosition < float3(0.0)) ||
           any(gridPosition > float3(dimensions - uint3(1u)));
}

public bool ddgiQueryIsTerrainInvalidated(U_ShadingInfo lighting,
                                           float3 worldPosition)
{
    return lighting.ddgi_invalidation_enabled != 0u &&
           all(worldPosition >= lighting.ddgi_invalidation_world_min) &&
           all(worldPosition <= lighting.ddgi_invalidation_world_max);
}

public DdgiQueryResult makeEmptyDdgiQueryResult()
{
    DdgiQueryResult result;
    result.irradiance = float3(0.0);
    result.accumulated_weight = 0.0;
    result.accumulated_base_weight = 0.0;
    result.mean_visibility = 0.0;
    result.contributing_probe_count = 0u;
    result.dominant_probe_index = 0u;
    result.dominant_probe_weight = 0.0;
    return result;
}

public DdgiProbeContribution getDdgiProbeContribution(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal,
    uint3 cageCorner)
{
    DdgiProbeContribution contribution;
    contribution.trustworthy = false;
    contribution.probe_index = 0u;
    contribution.irradiance = float3(0.0);
    contribution.actual_position = float3(0.0);
    contribution.nominal_position = float3(0.0);
    contribution.base_weight = 0.0;
    contribution.moment_visibility = 0.0;

    if (ddgiQueryUsesGlobalSky(lighting, worldPosition)) return contribution;

    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    uint3 dimensions = lighting.environment_probe_grid_dimensions;
    float3 gridScale = lighting.environment_probe_world_to_grid_scale;
    float3 gridPosition = worldPosition * gridScale;
    uint3 base = min(uint3(floor(gridPosition)), dimensions - uint3(2u));
    float3 fragmentInCell = saturate(gridPosition - float3(base));
    float3 spacingWorld = rcp(gridScale);
    uint3 cornerCoordinate = base + cageCorner;
    uint probeIndex = cornerCoordinate.x +
        dimensions.x * (cornerCoordinate.y +
                        dimensions.y * cornerCoordinate.z);
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
    if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID)
    {
        return contribution;
    }

    float3 actualPosition = metadata.actual_position_and_clearance.xyz;
    float3 nominalPosition = metadata.nominal_position_and_min_clearance.xyz;
    float positionWeight = ddgiRelocationAwarePositionWeight(
        fragmentInCell, float3(cageCorner), nominalPosition,
        actualPosition, spacingWorld);
    if (positionWeight <= DDGI_QUERY_WEIGHT_EPSILON) return contribution;

    float3 surfaceToProbe = actualPosition - worldPosition;
    float distanceSquared = dot(surfaceToProbe, surfaceToProbe);
    float3 surfaceToProbeDirection = distanceSquared > 1.0e-10
        ? surfaceToProbe * rsqrt(distanceSquared)
        : normal;
    float surfaceSideWeight = max(0.0, dot(normal, surfaceToProbeDirection));
    surfaceSideWeight *= surfaceSideWeight;
    if (surfaceSideWeight <= DDGI_QUERY_WEIGHT_EPSILON) return contribution;

    // Keep this in voxel units. The previous 0.25 / gridScale expression was
    // 0.25 probe spacing (8 voxels at the default spacing), which crossed
    // thin walls.
    float biasWorld = lighting.environment_probe_visibility_bias_world * 0.125;
    float3 biasedWorldPosition = worldPosition + normal * biasWorld;
    float3 probeToBiasedSurface = biasedWorldPosition - actualPosition;
    float distanceToSurface = length(probeToBiasedSurface);
    if (distanceToSurface <= 1.0e-6)
    {
        probeToBiasedSurface = -normal;
        distanceToSurface = 0.0;
    }
    float relocationDistance = distance(actualPosition, nominalPosition);
    float supportDistance = length(spacingWorld) + relocationDistance;
    if (distanceToSurface > supportDistance) return contribution;

    float3 probeToSurfaceDirection = normalize(probeToBiasedSurface);
    float2 moments = sampleDdgiDistanceMoments(
        probeIndex, probeToSurfaceDirection, lighting);
    contribution.trustworthy = true;
    contribution.probe_index = probeIndex;
    contribution.irradiance = sampleDdgiProbeIrradiance(
        probeIndex, normal, lighting);
    contribution.actual_position = actualPosition;
    contribution.nominal_position = nominalPosition;
    contribution.base_weight = positionWeight * surfaceSideWeight;
    contribution.moment_visibility = ddgiChebyshevVisibility(
        moments, distanceToSurface);
    return contribution;
}

public void accumulateDdgiContribution(
    inout DdgiQueryResult result, DdgiProbeContribution contribution,
    float visibility)
{
    if (!contribution.trustworthy) return;
    visibility = saturate(visibility);
    result.accumulated_base_weight += contribution.base_weight;
    result.mean_visibility += contribution.base_weight * visibility;
    float weight = contribution.base_weight * visibility;
    if (weight <= DDGI_QUERY_WEIGHT_EPSILON) return;
    result.irradiance += contribution.irradiance * weight;
    result.accumulated_weight += weight;
    result.contributing_probe_count += 1u;
    if (weight > result.dominant_probe_weight)
    {
        result.dominant_probe_weight = weight;
        result.dominant_probe_index = contribution.probe_index;
    }
}

public void finalizeDdgiQueryResult(inout DdgiQueryResult result)
{
    if (result.accumulated_base_weight > DDGI_QUERY_WEIGHT_EPSILON)
    {
        result.mean_visibility /= result.accumulated_base_weight;
    }
    if (result.accumulated_weight > DDGI_QUERY_WEIGHT_EPSILON)
    {
        result.irradiance /= result.accumulated_weight;
    }
    else
    {
        // Inside a ready volume, all rejected local probes mean zero. Never
        // resurrect an occluded cage through the global-sky fallback.
        result.irradiance = float3(0.0);
    }
    result.irradiance = max(result.irradiance, float3(0.0));
}

public DdgiQueryResult sampleDdgiDiffuseEnvironment(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    // Once terrain has changed, neither the active probe field nor its global-sky fallback is
    // trustworthy inside the edit's interpolation/relocation influence region.
    if (ddgiQueryIsTerrainInvalidated(lighting, worldPosition)) return result;
    if (ddgiQueryUsesGlobalSky(lighting, worldPosition))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        result.mean_visibility = 1.0;
        return result;
    }

    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                DdgiProbeContribution contribution =
                    getDdgiProbeContribution(
                        lighting, worldPosition, normal, uint3(x, y, z));
                accumulateDdgiContribution(
                    result, contribution, contribution.moment_visibility);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

public uint ddgiNearestNominalProbeIndex(U_ShadingInfo lighting,
                                         float3 worldPosition)
{
    uint3 dimensions = lighting.environment_probe_grid_dimensions;
    uint3 coordinate = uint3(clamp(
        round(worldPosition * lighting.environment_probe_world_to_grid_scale),
        float3(0.0), float3(dimensions - uint3(1u))));
    return coordinate.x + dimensions.x *
        (coordinate.y + dimensions.y * coordinate.z);
}

public float3 ddgiProbeStateDebugColor(U_ShadingInfo lighting,
                                       float3 worldPosition)
{
    if (ddgiQueryUsesGlobalSky(lighting, worldPosition))
    {
        return float3(0.0, 0.25, 1.0);
    }
    uint probeIndex = ddgiNearestNominalProbeIndex(lighting, worldPosition);
    uint state = ddgi_probe_metadata.data[probeIndex].state_and_reserved.x;
    if (state == DDGI_PROBE_STATE_VALID) return float3(0.0, 1.0, 0.0);
    if (state == DDGI_PROBE_STATE_RELOCATION_FAILED)
        return float3(1.0, 0.0, 1.0);
    return float3(0.1);
}

public float3 ddgiRelocationDebugColor(U_ShadingInfo lighting,
                                       float3 worldPosition)
{
    if (ddgiQueryUsesGlobalSky(lighting, worldPosition)) return float3(0.0);
    uint probeIndex = ddgiNearestNominalProbeIndex(lighting, worldPosition);
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
    if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID)
        return float3(1.0, 0.0, 1.0);
    float3 halfSpacing = 0.5 /
        lighting.environment_probe_world_to_grid_scale;
    float3 relocation =
        (metadata.actual_position_and_clearance.xyz -
         metadata.nominal_position_and_min_clearance.xyz) /
        max(halfSpacing, float3(1.0e-6));
    return float3(relocation.xy * 0.5 + 0.5,
                  saturate(length(relocation)));
}

public float3 ddgiDominantProbeDebugColor(DdgiQueryResult result)
{
    if (result.contributing_probe_count == 0u) return float3(0.0);
    uint value = result.dominant_probe_index * 747796405u + 2891336453u;
    value = ((value >> ((value >> 28u) + 4u)) ^ value) * 277803737u;
    value = (value >> 22u) ^ value;
    return float3(value & 255u, (value >> 8u) & 255u,
                  (value >> 16u) & 255u) / 255.0;
}

public float3 sampleDdgiIrradianceAtlasDebug(float2 screenUv)
{
    return max(float3(0.0),
               ddgi_irradiance_atlas.SampleLevel(screenUv, 0.0).xyz);
}

public float3 sampleDdgiVisibilityAtlasDebug(U_ShadingInfo lighting,
                                             float2 screenUv)
{
    float2 moments = ddgi_visibility_atlas.SampleLevel(screenUv, 0.0).xy;
    float supportDistance = length(
        rcp(lighting.environment_probe_world_to_grid_scale));
    float variance = max(0.0, moments.y - moments.x * moments.x);
    return float3(saturate(moments.x / max(supportDistance, 1.0e-6)),
                  saturate(variance /
                           max(supportDistance * supportDistance, 1.0e-6)),
                  0.0);
}
