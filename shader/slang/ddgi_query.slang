module ddgi_query;

import ddgi_octahedral;
import ddgi_types;
import ddgi_voxel_visibility;
import tracer_types;

static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_IRRADIANCE_STORED_SIDE = 10u;
static const uint DDGI_VISIBILITY_INTERIOR_SIDE = 16u;
static const uint DDGI_VISIBILITY_STORED_SIDE = 18u;
static const float DDGI_QUERY_WEIGHT_EPSILON = 1.0e-6;
static const float DDGI_MOMENT_VISIBILITY_MIN_CONFIDENCE = 0.125;

// Spatial-weight modes shared by the production terrain query and diagnostics.
// The low bit selects nominal versus relocation-aware position weights; the
// second bit selects same-side versus wrap surface weighting.
static const uint DDGI_SPATIAL_WEIGHT_RELOCATION_HARD = 0u;
static const uint DDGI_SPATIAL_WEIGHT_NOMINAL_HARD = 1u;
static const uint DDGI_SPATIAL_WEIGHT_RELOCATION_WRAP = 2u;
static const uint DDGI_SPATIAL_WEIGHT_NOMINAL_WRAP = 3u;
public static const uint DDGI_SPATIAL_WEIGHT_RELOCATION_NO_SURFACE = 4u;
public static const uint DDGI_SPATIAL_WEIGHT_NOMINAL_NO_SURFACE = 5u;
public static const uint DDGI_SPATIAL_WEIGHT_READBACK_STRIDE = 53u;
public static const uint DDGI_SPATIAL_WEIGHT_READBACK_PROBE_STRIDE = 6u;
public static const uint DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT = 6u;
public static const uint DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT = 8u;
static const uint DDGI_PROBE_REJECTION_INVALID_STATE = 1u;
static const uint DDGI_PROBE_REJECTION_POSITION_WEIGHT = 2u;
static const uint DDGI_PROBE_REJECTION_SURFACE_SIDE_WEIGHT = 4u;
static const uint DDGI_PROBE_REJECTION_SUPPORT = 8u;

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

struct DdgiProbeContribution
{
    bool trustworthy;
    uint probe_index;
    float3 irradiance;
    float3 actual_position;
    float3 nominal_position;
    float base_weight;
    float moment_visibility;
    float hard_visibility;
    uint probe_state;
    uint rejection_flags;
    float position_weight;
    float surface_side_weight;
    float distance_to_surface;
    float support_distance;
};

// The visibility-aware eight-probe algorithm consumes this narrow value instead of a renderer
// uniform. Consumer and transport adapters can therefore share the exact same implementation.
struct DdgiQueryInfo
{
    uint3 grid_dimensions;
    float3 world_to_grid_scale;
    float visibility_bias_world;
    uint irradiance_tile_columns;
    uint visibility_tile_columns;
    uint ready;
    uint geometry_revision;
    uint invalidation_enabled;
    float3 invalidation_world_min;
    float3 invalidation_world_max;
};

DdgiQueryInfo ddgiConsumerQueryInfo(U_ShadingInfo lighting)
{
    DdgiQueryInfo query;
    query.grid_dimensions = lighting.environment_probe_grid_dimensions;
    query.world_to_grid_scale = lighting.environment_probe_world_to_grid_scale;
    query.visibility_bias_world = lighting.environment_probe_visibility_bias_world;
    query.irradiance_tile_columns = lighting.ddgi_irradiance_tile_columns;
    query.visibility_tile_columns = lighting.ddgi_visibility_tile_columns;
    query.ready = lighting.ddgi_ready;
    query.geometry_revision = lighting.ddgi_geometry_revision;
    query.invalidation_enabled = lighting.ddgi_invalidation_enabled;
    query.invalidation_world_min = lighting.ddgi_invalidation_world_min;
    query.invalidation_world_max = lighting.ddgi_invalidation_world_max;
    return query;
}

DdgiQueryInfo ddgiTransportQueryInfo(U_DdgiTransportQueryInfo transport)
{
    DdgiQueryInfo query;
    query.grid_dimensions = transport.grid_dimensions;
    query.world_to_grid_scale = transport.world_to_grid_scale;
    query.visibility_bias_world = transport.visibility_bias_world;
    query.irradiance_tile_columns = transport.irradiance_tile_columns;
    query.visibility_tile_columns = transport.visibility_tile_columns;
    query.ready = transport.source_ready;
    query.geometry_revision = transport.geometry_revision;
    query.invalidation_enabled = 0u;
    query.invalidation_world_min = float3(0.0);
    query.invalidation_world_max = float3(0.0);
    return query;
}

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

float3 sampleDdgiProbeIrradiance(uint probeIndex, float3 normal,
                                 uint tileColumns,
                                 Sampler2D irradianceAtlas)
{
    float2 uv = ddgiAtlasUv(
        probeIndex, normal, tileColumns,
        DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE,
        irradianceAtlas);
    return max(float3(0.0),
               irradianceAtlas.SampleLevel(uv, 0.0).xyz);
}

public float3 sampleDdgiProbeIrradiance(uint probeIndex, float3 normal,
                                       U_ShadingInfo lighting)
{
    return sampleDdgiProbeIrradiance(
        probeIndex, normal, lighting.ddgi_irradiance_tile_columns,
        ddgi_irradiance_atlas);
}

// Diagnostic-only view of the local probe field. It averages the eight atlas
// tiles in the current nominal cell and deliberately bypasses all receiver
// position, surface-side, support, and visibility weights.
public float3 sampleDdgiRawCageIrradiance(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal)
{
    DdgiQueryInfo query = ddgiConsumerQueryInfo(lighting);
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    if (ddgiQueryUsesGlobalSky(query, worldPosition))
        return sampleDdgiGlobalSky(normal);
    uint3 dimensions = query.grid_dimensions;
    float3 gridPosition = worldPosition * query.world_to_grid_scale;
    uint3 base = min(uint3(floor(gridPosition)), dimensions - uint3(2u));
    float3 sum = float3(0.0);
    float count = 0.0;
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                uint3 coordinate = base + uint3(x, y, z);
                uint probeIndex = coordinate.x +
                    dimensions.x * (coordinate.y +
                                    dimensions.y * coordinate.z);
                DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
                if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID)
                    continue;
                sum += sampleDdgiProbeIrradiance(
                    probeIndex, normal, query.irradiance_tile_columns,
                    ddgi_irradiance_atlas);
                count += 1.0;
            }
        }
    }
    return count > 0.0 ? sum / count : float3(0.0);
}

float2 sampleDdgiDistanceMoments(uint probeIndex, float3 direction,
                                 DdgiQueryInfo query,
                                 Sampler2D visibilityAtlas)
{
    float2 uv = ddgiAtlasUv(
        probeIndex, direction, query.visibility_tile_columns,
        DDGI_VISIBILITY_INTERIOR_SIDE, DDGI_VISIBILITY_STORED_SIDE,
        visibilityAtlas);
    return visibilityAtlas.SampleLevel(uv, 0.0).xy;
}

float2 sampleDdgiDistanceMoments(uint probeIndex, float3 direction,
                                 DdgiQueryInfo query)
{
    return sampleDdgiDistanceMoments(
        probeIndex, direction, query, ddgi_visibility_atlas);
}

public float ddgiChebyshevVisibility(float2 moments, float distanceToSurface)
{
    if (distanceToSurface <= moments.x) return 1.0;
    float variance = max(0.0, moments.y - moments.x * moments.x);
    float distancePastMean = distanceToSurface - moments.x;
    float probability = variance /
        max(variance + distancePastMean * distancePastMean, 1.0e-8);
    // A tiny statistical visibility must not survive normalization as a fully
    // lit probe. Reject low-confidence support instead of adding a positive
    // visibility floor, which would guarantee light through occluders.
    float visibility = probability * probability * probability;
    return visibility >= DDGI_MOMENT_VISIBILITY_MIN_CONFIDENCE
        ? visibility
        : 0.0;
}

public float ddgiMomentNormalizationWeight(
    float accumulatedBaseWeight, float accumulatedWeight)
{
    return max(
        accumulatedWeight,
        accumulatedBaseWeight * DDGI_MOMENT_VISIBILITY_MIN_CONFIDENCE);
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

float ddgiNominalTrilinearPositionWeight(float3 fragmentInCell,
                                          float3 corner)
{
    float3 axial = lerp(float3(1.0) - fragmentInCell,
                        fragmentInCell, corner);
    return axial.x * axial.y * axial.z;
}

float3 ddgiNormalizedSurfaceNormal(float3 surfaceNormal)
{
    float normalLengthSquared = dot(surfaceNormal, surfaceNormal);
    return normalLengthSquared > 1.0e-8
        ? surfaceNormal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
}

bool ddgiQueryUsesGlobalSky(DdgiQueryInfo query,
                            float3 worldPosition)
{
    uint3 dimensions = query.grid_dimensions;
    float3 gridScale = query.world_to_grid_scale;
    if (query.ready == 0u ||
        any(dimensions < uint3(2u)) || any(gridScale <= float3(0.0)))
    {
        return true;
    }
    float3 gridPosition = worldPosition * gridScale;
    return any(gridPosition < float3(0.0)) ||
           any(gridPosition > float3(dimensions - uint3(1u)));
}

bool ddgiQueryUsesGlobalSky(U_ShadingInfo lighting,
                            float3 worldPosition)
{
    return ddgiQueryUsesGlobalSky(
        ddgiConsumerQueryInfo(lighting), worldPosition);
}

bool ddgiQueryIsTerrainInvalidated(DdgiQueryInfo query,
                                    float3 worldPosition)
{
    return query.invalidation_enabled != 0u &&
           all(worldPosition >= query.invalidation_world_min) &&
           all(worldPosition <= query.invalidation_world_max);
}

bool ddgiQueryIsTerrainInvalidated(U_ShadingInfo lighting,
                                   float3 worldPosition)
{
    return ddgiQueryIsTerrainInvalidated(
        ddgiConsumerQueryInfo(lighting), worldPosition);
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

DdgiProbeContribution getDdgiUnoccludedSpatialWeightProbeContributionAt(
    DdgiQueryInfo query, float3 worldPosition,
    float3 positionWeightWorldPosition, float3 surfaceNormal,
    uint3 cageCorner, uint spatialWeightMode, Sampler2D irradianceAtlas)
{
    DdgiProbeContribution contribution;
    contribution.trustworthy = false;
    contribution.probe_index = 0u;
    contribution.irradiance = float3(0.0);
    contribution.actual_position = float3(0.0);
    contribution.nominal_position = float3(0.0);
    contribution.base_weight = 0.0;
    contribution.moment_visibility = 0.0;
    contribution.hard_visibility = 0.0;
    contribution.probe_state = 0u;
    contribution.rejection_flags = 0u;
    contribution.position_weight = 0.0;
    contribution.surface_side_weight = 0.0;
    contribution.distance_to_surface = 0.0;
    contribution.support_distance = 0.0;

    if (ddgiQueryUsesGlobalSky(query, worldPosition)) return contribution;

    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    uint3 dimensions = query.grid_dimensions;
    float3 gridScale = query.world_to_grid_scale;
    float3 gridPosition = worldPosition * gridScale;
    uint3 base = min(uint3(floor(gridPosition)), dimensions - uint3(2u));
    float3 positionWeightGridPosition =
        positionWeightWorldPosition * gridScale;
    float3 fragmentInCell =
        saturate(positionWeightGridPosition - float3(base));
    float3 spacingWorld = rcp(gridScale);
    uint3 cornerCoordinate = base + cageCorner;
    uint probeIndex = cornerCoordinate.x +
        dimensions.x * (cornerCoordinate.y +
                        dimensions.y * cornerCoordinate.z);
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
    contribution.probe_index = probeIndex;
    contribution.probe_state = metadata.state_and_reserved.x;
    contribution.actual_position = metadata.actual_position_and_clearance.xyz;
    contribution.nominal_position = metadata.nominal_position_and_min_clearance.xyz;
    if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID)
    {
        contribution.rejection_flags |= DDGI_PROBE_REJECTION_INVALID_STATE;
        return contribution;
    }

    float3 actualPosition = metadata.actual_position_and_clearance.xyz;
    float3 nominalPosition = metadata.nominal_position_and_min_clearance.xyz;
    float positionWeight = (spatialWeightMode & 1u) != 0u
        ? ddgiNominalTrilinearPositionWeight(
            fragmentInCell, float3(cageCorner))
        : ddgiRelocationAwarePositionWeight(
            fragmentInCell, float3(cageCorner), nominalPosition,
            actualPosition, spacingWorld);
    contribution.position_weight = positionWeight;
    if (positionWeight <= DDGI_QUERY_WEIGHT_EPSILON)
    {
        contribution.rejection_flags |= DDGI_PROBE_REJECTION_POSITION_WEIGHT;
        return contribution;
    }

    float3 surfaceToProbe = actualPosition - positionWeightWorldPosition;
    float distanceSquared = dot(surfaceToProbe, surfaceToProbe);
    float3 surfaceToProbeDirection = distanceSquared > 1.0e-10
        ? surfaceToProbe * rsqrt(distanceSquared)
        : normal;
    float surfaceAlignment = dot(normal, surfaceToProbeDirection);
    float surfaceSideWeight;
    if ((spatialWeightMode & 4u) != 0u)
    {
        surfaceSideWeight = 1.0;
    }
    else if ((spatialWeightMode & 2u) != 0u)
    {
        // RTXGI's wrap-shading component only. Keep the existing support and
        // exact/moment visibility gates unchanged for this diagnosis.
        float wrapShading = (surfaceAlignment + 1.0) * 0.5;
        surfaceSideWeight = wrapShading * wrapShading + 0.2;
    }
    else
    {
        surfaceSideWeight = sqrt(max(0.0, surfaceAlignment));
    }
    contribution.surface_side_weight = surfaceSideWeight;
    if (surfaceSideWeight <= DDGI_QUERY_WEIGHT_EPSILON)
    {
        contribution.rejection_flags |= DDGI_PROBE_REJECTION_SURFACE_SIDE_WEIGHT;
        return contribution;
    }

    float biasWorld = max(0.0, query.visibility_bias_world);
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
    contribution.distance_to_surface = distanceToSurface;
    contribution.support_distance = supportDistance;
    if (distanceToSurface > supportDistance)
    {
        contribution.rejection_flags |= DDGI_PROBE_REJECTION_SUPPORT;
        return contribution;
    }

    contribution.trustworthy = true;
    contribution.base_weight = positionWeight * surfaceSideWeight;
    contribution.moment_visibility = 1.0;
    contribution.hard_visibility = 1.0;
    contribution.irradiance = sampleDdgiProbeIrradiance(
        probeIndex, normal, query.irradiance_tile_columns, irradianceAtlas);
    return contribution;
}

DdgiProbeContribution getDdgiMomentSpatialWeightProbeContributionAt(
    DdgiQueryInfo query, float3 worldPosition,
    float3 positionWeightWorldPosition, float3 surfaceNormal,
    uint3 cageCorner, uint spatialWeightMode, Sampler2D irradianceAtlas)
{
    DdgiProbeContribution contribution =
        getDdgiUnoccludedSpatialWeightProbeContributionAt(
            query, worldPosition, positionWeightWorldPosition, surfaceNormal,
            cageCorner, spatialWeightMode, irradianceAtlas);
    if (!contribution.trustworthy) return contribution;

    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    float3 biasedWorldPosition = worldPosition +
        normal * max(0.0, query.visibility_bias_world);
    float3 probeToBiasedSurface =
        biasedWorldPosition - contribution.actual_position;
    if (dot(probeToBiasedSurface, probeToBiasedSurface) <= 1.0e-12)
        probeToBiasedSurface = -normal;
    float2 moments = sampleDdgiDistanceMoments(
        contribution.probe_index, normalize(probeToBiasedSurface), query);
    contribution.moment_visibility = ddgiChebyshevVisibility(
        moments, contribution.distance_to_surface);
    return contribution;
}

DdgiProbeContribution getDdgiMomentExactSpatialWeightProbeContributionAt(
    DdgiQueryInfo query, float3 worldPosition,
    float3 positionWeightWorldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint3 cageCorner,
    uint spatialWeightMode, Sampler2D irradianceAtlas)
{
    DdgiProbeContribution contribution =
        getDdgiMomentSpatialWeightProbeContributionAt(
            query, worldPosition, positionWeightWorldPosition, surfaceNormal,
            cageCorner, spatialWeightMode, irradianceAtlas);
    if (!contribution.trustworthy) return contribution;
    contribution.hard_visibility = ddgiVoxelSegmentVisibility(
        hardVisibilityWorldPosition, contribution.actual_position,
        query.geometry_revision);
    return contribution;
}

DdgiProbeContribution getDdgiMomentProbeContribution(
    DdgiQueryInfo query, float3 worldPosition, float3 surfaceNormal,
    uint3 cageCorner, Sampler2D irradianceAtlas)
{
    return getDdgiMomentSpatialWeightProbeContributionAt(
        query, worldPosition, worldPosition, surfaceNormal, cageCorner,
        DDGI_SPATIAL_WEIGHT_RELOCATION_HARD, irradianceAtlas);
}

DdgiProbeContribution getDdgiMomentExactProbeContribution(
    DdgiQueryInfo query, float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint3 cageCorner,
    Sampler2D irradianceAtlas)
{
    return getDdgiMomentExactSpatialWeightProbeContributionAt(
        query, worldPosition, worldPosition, surfaceNormal,
        hardVisibilityWorldPosition, cageCorner,
        DDGI_SPATIAL_WEIGHT_RELOCATION_HARD, irradianceAtlas);
}

DdgiProbeContribution getDdgiMomentExactProbeContributionFromAtlases(
    DdgiQueryInfo query, float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint3 cageCorner,
    Sampler2D irradianceAtlas, Sampler2D visibilityAtlas)
{
    DdgiProbeContribution contribution =
        getDdgiUnoccludedSpatialWeightProbeContributionAt(
            query, worldPosition, worldPosition, surfaceNormal,
            cageCorner, DDGI_SPATIAL_WEIGHT_RELOCATION_HARD,
            irradianceAtlas);
    if (!contribution.trustworthy) return contribution;

    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    float3 biasedWorldPosition = worldPosition +
        normal * max(0.0, query.visibility_bias_world);
    float3 probeToBiasedSurface =
        biasedWorldPosition - contribution.actual_position;
    if (dot(probeToBiasedSurface, probeToBiasedSurface) <= 1.0e-12)
        probeToBiasedSurface = -normal;
    float2 moments = sampleDdgiDistanceMoments(
        contribution.probe_index, normalize(probeToBiasedSurface),
        query, visibilityAtlas);
    contribution.moment_visibility = ddgiChebyshevVisibility(
        moments, contribution.distance_to_surface);
    contribution.hard_visibility = ddgiVoxelSegmentVisibility(
        hardVisibilityWorldPosition, contribution.actual_position,
        query.geometry_revision);
    return contribution;
}

DdgiProbeContribution getDdgiMomentExactProbeContribution(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint3 cageCorner)
{
    return getDdgiMomentExactProbeContribution(
        ddgiConsumerQueryInfo(lighting), worldPosition, surfaceNormal,
        hardVisibilityWorldPosition, cageCorner, ddgi_irradiance_atlas);
}

DdgiProbeContribution getDdgiMomentExactSpatialWeightProbeContribution(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint3 cageCorner,
    uint spatialWeightMode)
{
    return getDdgiMomentExactSpatialWeightProbeContributionAt(
        ddgiConsumerQueryInfo(lighting), worldPosition, worldPosition,
        surfaceNormal, hardVisibilityWorldPosition, cageCorner,
        spatialWeightMode, ddgi_irradiance_atlas);
}

DdgiProbeContribution getDdgiTerrainSmoothMomentExactProbeContribution(
    U_ShadingInfo lighting, float3 receiverWorldPosition,
    float3 positionWeightWorldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint3 cageCorner)
{
    return getDdgiMomentExactSpatialWeightProbeContributionAt(
        ddgiConsumerQueryInfo(lighting), receiverWorldPosition,
        positionWeightWorldPosition,
        surfaceNormal, hardVisibilityWorldPosition, cageCorner,
        DDGI_SPATIAL_WEIGHT_NOMINAL_HARD, ddgi_irradiance_atlas);
}

// Diagnostic-only adapter for an unoccluded query. Keep all normal probe
// validity, relocation, support, and irradiance sampling in this module while
// suppressing only the consumer visibility terms.
DdgiProbeContribution getDdgiUnoccludedProbeContribution(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal,
    uint3 cageCorner)
{
    return getDdgiUnoccludedSpatialWeightProbeContributionAt(
        ddgiConsumerQueryInfo(lighting), worldPosition, worldPosition,
        surfaceNormal, cageCorner, DDGI_SPATIAL_WEIGHT_RELOCATION_HARD,
        ddgi_irradiance_atlas);
}

void accumulateDdgiContribution(
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

// Diagnostic-only averaging that removes the trilinear position and surface-side
// weights while retaining the same trustworthy-probe and support gates.
void accumulateDdgiEqualWeightContribution(
    inout DdgiQueryResult result, DdgiProbeContribution contribution)
{
    if (!contribution.trustworthy) return;
    result.accumulated_base_weight += 1.0;
    result.mean_visibility += 1.0;
    result.irradiance += contribution.irradiance;
    result.accumulated_weight += 1.0;
    result.contributing_probe_count += 1u;
    if (result.dominant_probe_weight < 1.0)
    {
        result.dominant_probe_weight = 1.0;
        result.dominant_probe_index = contribution.probe_index;
    }
}

void finalizeDdgiQueryResultWithWeight(
    inout DdgiQueryResult result, float normalizationWeight)
{
    if (result.accumulated_base_weight > DDGI_QUERY_WEIGHT_EPSILON)
    {
        result.mean_visibility /= result.accumulated_base_weight;
    }
    if (normalizationWeight > DDGI_QUERY_WEIGHT_EPSILON)
    {
        result.irradiance /= normalizationWeight;
    }
    else
    {
        // Inside a ready volume, all rejected local probes mean zero. Never
        // resurrect an occluded cage through the global-sky fallback.
        result.irradiance = float3(0.0);
    }
    result.irradiance = max(result.irradiance, float3(0.0));
}

void finalizeDdgiQueryResult(inout DdgiQueryResult result)
{
    finalizeDdgiQueryResultWithWeight(result, result.accumulated_weight);
}

void finalizeDdgiMomentQueryResult(inout DdgiQueryResult result)
{
    // Per-probe confidence filtering is not sufficient when the only surviving
    // probe has negligible spatial support: normalizing by that tiny weight
    // turns a statistical outlier into full irradiance. Cap that amplification
    // at the same confidence already required of an individual moment sample.
    float normalizationWeight = ddgiMomentNormalizationWeight(
        result.accumulated_base_weight, result.accumulated_weight);
    finalizeDdgiQueryResultWithWeight(result, normalizationWeight);
}

// Semantic diagnostic queries keep cage traversal and contribution assembly beside production.
// Their names describe the isolated behavior; callers cannot rebuild the weighting algorithm.
public DdgiQueryResult sampleDdgiExactTerrainReference(
    U_ShadingInfo lighting, float3 worldPosition,
    float3 positionWeightWorldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
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
                    getDdgiTerrainSmoothMomentExactProbeContribution(
                        lighting, worldPosition,
                        positionWeightWorldPosition, normal,
                        hardVisibilityWorldPosition, uint3(x, y, z));
                float visibility = contribution.trustworthy
                    ? contribution.hard_visibility
                    : 0.0;
                accumulateDdgiContribution(result, contribution, visibility);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

public DdgiQueryResult sampleDdgiUnoccludedTerrainReference(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
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
                    getDdgiUnoccludedProbeContribution(
                        lighting, worldPosition, normal, uint3(x, y, z));
                accumulateDdgiContribution(result, contribution, 1.0);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

public DdgiQueryResult sampleDdgiEqualWeightTerrainReference(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
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
                    getDdgiUnoccludedProbeContribution(
                        lighting, worldPosition, normal, uint3(x, y, z));
                accumulateDdgiEqualWeightContribution(result, contribution);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

public DdgiQueryResult sampleDdgiSpatialWeightTerrainReference(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition, uint spatialWeightMode)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
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
                    getDdgiMomentExactSpatialWeightProbeContribution(
                        lighting, worldPosition, normal,
                        hardVisibilityWorldPosition, uint3(x, y, z),
                        spatialWeightMode);
                float visibility = contribution.trustworthy
                    ? contribution.hard_visibility
                    : 0.0;
                accumulateDdgiContribution(result, contribution, visibility);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

// Diagnostic serialization is concrete because there is one real GPU readback consumer. Keeping
// it here prevents the renderer from reconstructing cage traversal and alternate-weight queries.
public void writeDdgiSpatialWeightDiagnostics(
    RWStructuredBuffer<float4> output, uint receiverBase,
    U_ShadingInfo lighting, float3 receiverWorldPosition,
    float3 surfaceNormal, float3 hardVisibilityWorldPosition)
{
    bool usesGlobalSky = ddgiQueryUsesGlobalSky(lighting, receiverWorldPosition);
    float queryStatus = 1.0 + (lighting.ddgi_ready != 0u ? 2.0 : 0.0) +
        (usesGlobalSky ? 4.0 : 0.0);
    output[receiverBase] = float4(receiverWorldPosition, queryStatus);

    DdgiQueryResult exactResult = makeEmptyDdgiQueryResult();
    DdgiQueryResult consumerResult = makeEmptyDdgiQueryResult();
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                uint3 cageCorner = uint3(x, y, z);
                DdgiProbeContribution contribution =
                    getDdgiMomentExactProbeContribution(
                        lighting, receiverWorldPosition, surfaceNormal,
                        hardVisibilityWorldPosition, cageCorner);
                DdgiProbeContribution nominalContribution =
                    getDdgiMomentExactSpatialWeightProbeContribution(
                        lighting, receiverWorldPosition, surfaceNormal,
                        hardVisibilityWorldPosition, cageCorner,
                        DDGI_SPATIAL_WEIGHT_NOMINAL_HARD);
                DdgiProbeContribution wrapContribution =
                    getDdgiMomentExactSpatialWeightProbeContribution(
                        lighting, receiverWorldPosition, surfaceNormal,
                        hardVisibilityWorldPosition, cageCorner,
                        DDGI_SPATIAL_WEIGHT_RELOCATION_WRAP);
                DdgiProbeContribution nominalWrapContribution =
                    getDdgiMomentExactSpatialWeightProbeContribution(
                        lighting, receiverWorldPosition, surfaceNormal,
                        hardVisibilityWorldPosition, cageCorner,
                        DDGI_SPATIAL_WEIGHT_NOMINAL_WRAP);
                float hardVisibility = contribution.trustworthy
                    ? contribution.hard_visibility
                    : 0.0;
                float consumerVisibility = contribution.trustworthy
                    ? hardVisibility * contribution.moment_visibility
                    : 0.0;
                accumulateDdgiContribution(
                    exactResult, contribution, hardVisibility);
                accumulateDdgiContribution(
                    consumerResult, contribution, consumerVisibility);

                uint probeIndex = x + 2u * (y + 2u * z);
                uint probeBase = receiverBase + 1u +
                    probeIndex * DDGI_SPATIAL_WEIGHT_READBACK_PROBE_STRIDE;
                output[probeBase] = float4(
                    float(contribution.probe_index),
                    float(contribution.probe_state),
                    contribution.trustworthy ? 1.0 : 0.0,
                    float(contribution.rejection_flags));
                output[probeBase + 1u] = float4(
                    contribution.actual_position,
                    contribution.position_weight);
                output[probeBase + 2u] = float4(
                    contribution.nominal_position,
                    contribution.surface_side_weight);
                output[probeBase + 3u] = float4(
                    contribution.irradiance,
                    contribution.moment_visibility);
                output[probeBase + 4u] = float4(
                    contribution.hard_visibility,
                    contribution.distance_to_surface,
                    contribution.support_distance,
                    contribution.base_weight);
                output[probeBase + 5u] = float4(
                    contribution.base_weight * hardVisibility,
                    nominalContribution.base_weight *
                        (nominalContribution.trustworthy
                            ? nominalContribution.hard_visibility : 0.0),
                    wrapContribution.base_weight *
                        (wrapContribution.trustworthy
                            ? wrapContribution.hard_visibility : 0.0),
                    nominalWrapContribution.base_weight *
                        (nominalWrapContribution.trustworthy
                            ? nominalWrapContribution.hard_visibility : 0.0));
            }
        }
    }
    finalizeDdgiQueryResult(exactResult);
    finalizeDdgiQueryResult(consumerResult);
    output[receiverBase + 49u] = float4(
        exactResult.irradiance, exactResult.accumulated_weight);
    output[receiverBase + 50u] = float4(
        consumerResult.irradiance, consumerResult.accumulated_weight);
    output[receiverBase + 51u] = float4(
        exactResult.accumulated_base_weight,
        exactResult.mean_visibility,
        float(exactResult.contributing_probe_count),
        float(exactResult.dominant_probe_index));
    output[receiverBase + 52u] = float4(
        consumerResult.accumulated_base_weight,
        consumerResult.mean_visibility,
        float(consumerResult.contributing_probe_count),
        float(consumerResult.dominant_probe_index));
}

DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(
    DdgiQueryInfo query, Sampler2D irradianceAtlas,
    float3 worldPosition, float3 surfaceNormal)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    // Once terrain has changed, neither the active probe field nor its global-sky fallback is
    // trustworthy inside the edit's interpolation/relocation influence region.
    if (ddgiQueryIsTerrainInvalidated(query, worldPosition)) return result;
    if (ddgiQueryUsesGlobalSky(query, worldPosition))
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
                    getDdgiMomentProbeContribution(
                        query, worldPosition, normal, uint3(x, y, z),
                        irradianceAtlas);
                accumulateDdgiContribution(
                    result, contribution, contribution.moment_visibility);
            }
        }
    }
    finalizeDdgiMomentQueryResult(result);
    return result;
}

DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(
    DdgiQueryInfo query, Sampler2D irradianceAtlas,
    float3 receiverWorldPosition, float3 positionWeightWorldPosition,
    float3 surfaceNormal)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    if (ddgiQueryIsTerrainInvalidated(query, receiverWorldPosition))
        return result;
    if (ddgiQueryUsesGlobalSky(query, receiverWorldPosition))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        result.mean_visibility = 1.0;
        return result;
    }

    DdgiQueryResult canonicalResult = sampleDdgiDiffuseEnvironmentFromAtlas(
        query, irradianceAtlas, receiverWorldPosition, normal);
    if (all(canonicalResult.irradiance <=
            float3(DDGI_QUERY_WEIGHT_EPSILON)))
        return canonicalResult;

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
                    getDdgiMomentSpatialWeightProbeContributionAt(
                        query, receiverWorldPosition,
                        positionWeightWorldPosition, normal,
                        uint3(x, y, z), DDGI_SPATIAL_WEIGHT_NOMINAL_HARD,
                        irradianceAtlas);
                accumulateDdgiContribution(
                    result, contribution, contribution.moment_visibility);
            }
        }
    }
    finalizeDdgiMomentQueryResult(result);
    if (result.accumulated_weight <= DDGI_QUERY_WEIGHT_EPSILON ||
        all(result.irradiance <= float3(DDGI_QUERY_WEIGHT_EPSILON)))
    {
        // The continuous spatial basis must not split one voxel into lit and
        // zero-weight pixels. Retry the same visibility-safe receiver at its
        // canonical coordinate; this remains fail-closed if that query also
        // has no trustworthy contribution.
        return canonicalResult;
    }
    return result;
}

public DdgiQueryResult sampleDdgiTerrainSmoothEnvironment(
    U_ShadingInfo lighting, float3 receiverWorldPosition,
    float3 positionWeightWorldPosition, float3 surfaceNormal)
{
    return sampleDdgiTerrainSmoothEnvironmentFromAtlas(
        ddgiConsumerQueryInfo(lighting), ddgi_irradiance_atlas,
        receiverWorldPosition, positionWeightWorldPosition,
        surfaceNormal);
}

public DdgiQueryResult sampleDdgiDiffuseEnvironment(
    U_ShadingInfo lighting, float3 worldPosition, float3 surfaceNormal)
{
    return sampleDdgiDiffuseEnvironmentFromAtlas(
        ddgiConsumerQueryInfo(lighting), ddgi_irradiance_atlas,
        worldPosition, surfaceNormal);
}

DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(
    DdgiQueryInfo query, Sampler2D irradianceAtlas,
    Sampler2D visibilityAtlas,
    float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = ddgiNormalizedSurfaceNormal(surfaceNormal);
    if (ddgiQueryUsesGlobalSky(query, worldPosition))
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
                    getDdgiMomentExactProbeContributionFromAtlases(
                        query, worldPosition, normal,
                        hardVisibilityWorldPosition, uint3(x, y, z),
                        irradianceAtlas, visibilityAtlas);
                accumulateDdgiContribution(
                    result, contribution,
                    contribution.moment_visibility *
                        contribution.hard_visibility);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

public DdgiQueryResult sampleDdgiTransportSource(
    U_DdgiTransportQueryInfo transport, Sampler2D sourceIrradianceAtlas,
    Sampler2D sourceVisibilityAtlas,
    float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    DdgiQueryInfo query = ddgiTransportQueryInfo(transport);
    return sampleDdgiTransportEnvironmentFromAtlas(
        query, sourceIrradianceAtlas, sourceVisibilityAtlas,
        worldPosition, surfaceNormal,
        hardVisibilityWorldPosition);
}

public DdgiQueryResult sampleDdgiTransportAtlas0(
    U_DdgiTransportQueryInfo transport,
    float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    return sampleDdgiTransportSource(
        transport, ddgi_irradiance_atlas, ddgi_visibility_atlas,
        worldPosition, surfaceNormal,
        hardVisibilityWorldPosition);
}

public uint ddgiNearestNominalProbeIndex(U_ShadingInfo lighting,
                                         float3 worldPosition)
{
    DdgiQueryInfo query = ddgiConsumerQueryInfo(lighting);
    uint3 dimensions = query.grid_dimensions;
    uint3 coordinate = uint3(clamp(
        round(worldPosition * query.world_to_grid_scale),
        float3(0.0), float3(dimensions - uint3(1u))));
    return coordinate.x + dimensions.x *
        (coordinate.y + dimensions.y * coordinate.z);
}

public float3 ddgiProbeStateDebugColor(U_ShadingInfo lighting,
                                       float3 worldPosition)
{
    if (ddgiQueryUsesGlobalSky(ddgiConsumerQueryInfo(lighting), worldPosition))
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
    if (ddgiQueryUsesGlobalSky(ddgiConsumerQueryInfo(lighting), worldPosition))
        return float3(0.0);
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
