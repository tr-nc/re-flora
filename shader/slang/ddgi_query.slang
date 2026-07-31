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

public struct DdgiQueryResult
{
    public float3 irradiance;
    public float accumulated_weight;
    public float accumulated_moment_visibility;
    public uint contributing_probe_count;
    public uint dominant_probe_index;
    public float dominant_probe_weight;
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

float3 sampleDdgiGlobalSky(float3 normal)
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

float3 sampleDdgiProbeIrradiance(uint probeIndex, float3 normal,
                                 U_ShadingInfo lighting)
{
    float2 uv = ddgiAtlasUv(
        probeIndex, normal, lighting.ddgi_irradiance_tile_columns,
        DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE,
        ddgi_irradiance_atlas);
    return max(float3(0.0),
               ddgi_irradiance_atlas.SampleLevel(uv, 0.0).xyz);
}

float2 sampleDdgiDistanceMoments(uint probeIndex, float3 direction,
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

public DdgiQueryResult sampleDdgiDiffuseEnvironment(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal)
{
    DdgiQueryResult result;
    result.irradiance = float3(0.0);
    result.accumulated_weight = 0.0;
    result.accumulated_moment_visibility = 0.0;
    result.contributing_probe_count = 0u;
    result.dominant_probe_index = 0u;
    result.dominant_probe_weight = 0.0;

    float normalLengthSquared = dot(surfaceNormal, surfaceNormal);
    float3 normal = normalLengthSquared > 1.0e-8
        ? surfaceNormal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
    uint3 dimensions = lighting.environment_probe_grid_dimensions;
    float3 gridScale = lighting.environment_probe_world_to_grid_scale;
    if (lighting.environment_lighting_backend_ready == 0u ||
        any(dimensions < uint3(2u)) || any(gridScale <= float3(0.0)))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        return result;
    }

    float3 gridMaximum = float3(dimensions - uint3(1u));
    float3 gridPosition = worldPosition * gridScale;
    if (any(gridPosition < float3(0.0)) || any(gridPosition > gridMaximum))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        return result;
    }

    uint3 base = min(uint3(floor(gridPosition)), dimensions - uint3(2u));
    float3 fragmentInCell = saturate(gridPosition - float3(base));
    float3 spacingWorld = rcp(gridScale);
    float biasWorld = 0.25 /
        max(max(gridScale.x, gridScale.y), gridScale.z);
    float3 biasedWorldPosition = worldPosition + normal * biasWorld;

    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                uint3 cornerCoordinate = base + uint3(x, y, z);
                uint probeIndex = cornerCoordinate.x +
                    dimensions.x *
                        (cornerCoordinate.y + dimensions.y *
                                                  cornerCoordinate.z);
                DdgiProbeMetadata metadata =
                    ddgi_probe_metadata.data[probeIndex];
                if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID)
                {
                    continue;
                }

                float3 actualPosition =
                    metadata.actual_position_and_clearance.xyz;
                float3 nominalPosition =
                    metadata.nominal_position_and_min_clearance.xyz;
                float positionWeight = ddgiRelocationAwarePositionWeight(
                    fragmentInCell, float3(x, y, z), nominalPosition,
                    actualPosition, spacingWorld);
                if (positionWeight <= DDGI_QUERY_WEIGHT_EPSILON) continue;

                float3 surfaceToProbe = actualPosition - worldPosition;
                float distanceSquared = dot(surfaceToProbe, surfaceToProbe);
                float3 surfaceToProbeDirection = distanceSquared > 1.0e-10
                    ? surfaceToProbe * rsqrt(distanceSquared)
                    : normal;
                float surfaceSideWeight =
                    max(0.0, dot(normal, surfaceToProbeDirection));
                surfaceSideWeight *= surfaceSideWeight;
                if (surfaceSideWeight <= DDGI_QUERY_WEIGHT_EPSILON) continue;

                float3 probeToBiasedSurface =
                    biasedWorldPosition - actualPosition;
                float distanceToSurface = length(probeToBiasedSurface);
                if (distanceToSurface <= 1.0e-6)
                {
                    probeToBiasedSurface = -normal;
                    distanceToSurface = 0.0;
                }
                float relocationDistance =
                    distance(actualPosition, nominalPosition);
                float supportDistance = length(spacingWorld) +
                                        relocationDistance;
                if (distanceToSurface > supportDistance) continue;

                float3 probeToSurfaceDirection =
                    normalize(probeToBiasedSurface);
                float2 moments = sampleDdgiDistanceMoments(
                    probeIndex, probeToSurfaceDirection, lighting);
                float momentVisibility = ddgiChebyshevVisibility(
                    moments, distanceToSurface);
                float weight = positionWeight * surfaceSideWeight *
                               momentVisibility;
                if (weight <= DDGI_QUERY_WEIGHT_EPSILON) continue;

                float3 irradiance = sampleDdgiProbeIrradiance(
                    probeIndex, normal, lighting);
                result.irradiance += irradiance * weight;
                result.accumulated_weight += weight;
                result.accumulated_moment_visibility +=
                    momentVisibility * weight;
                result.contributing_probe_count += 1u;
                if (weight > result.dominant_probe_weight)
                {
                    result.dominant_probe_weight = weight;
                    result.dominant_probe_index = probeIndex;
                }
            }
        }
    }

    if (result.accumulated_weight > DDGI_QUERY_WEIGHT_EPSILON)
    {
        result.irradiance /= result.accumulated_weight;
        result.accumulated_moment_visibility /= result.accumulated_weight;
    }
    else
    {
        // Inside a ready volume, all rejected local probes mean zero. Never
        // resurrect an occluded cage through the global-sky fallback.
        result.irradiance = float3(0.0);
    }
    result.irradiance = max(result.irradiance, float3(0.0));
    return result;
}
