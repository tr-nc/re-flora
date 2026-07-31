module environment_lighting;

import tracer_types;

struct EnvironmentProbeSummary
{
    float4 original_position_visibility;
    float4 sample_position_confidence;
    uint4 revisions_state;
    uint4 update_info;
};

struct EnvironmentProbeCoefficients
{
    float4 coefficients[9];
};

struct EnvironmentProbeVisibility
{
    uint packed_hit_distances[32];
    uint packed_distance_moments[64];
};

struct B_EnvironmentProbeSummaries
{
    EnvironmentProbeSummary data[];
};

struct B_EnvironmentProbeCoefficients
{
    EnvironmentProbeCoefficients data[];
};

struct B_EnvironmentProbeVisibility
{
    EnvironmentProbeVisibility data[];
};

[[vk::binding(18, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeCoefficients, Std430DataLayout>
    environment_probe_coefficients;

[[vk::binding(19, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

[[vk::binding(20, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeVisibility, Std430DataLayout>
    environment_probe_visibility;

static const uint ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE = 8u;

// Sloan real-SH ordering adapted to the project's Y-up world:
// [Y00, Y1-1(z), Y10(y), Y11(x), Y2-2(xz), Y2-1(zy),
//  Y20(3y^2-1), Y21(xy), Y22(x^2-z^2)].
float environmentShBasis(uint index, float3 direction)
{
    float x = direction.x;
    float y = direction.y;
    float z = direction.z;
    switch (index)
    {
        case 0u: return 0.2820948;
        case 1u: return 0.48860252 * z;
        case 2u: return 0.48860252 * y;
        case 3u: return 0.48860252 * x;
        case 4u: return 1.0925485 * x * z;
        case 5u: return 1.0925485 * z * y;
        case 6u: return 0.31539157 * (3.0 * y * y - 1.0);
        case 7u: return 1.0925485 * x * y;
        default: return 0.54627424 * (x * x - z * z);
    }
}

float3 evaluateGlobalEnvironmentIrradiance(U_ShadingInfo lighting,
                                            float3 normal)
{
    float3 irradiance = float3(0.0);
    irradiance += lighting.environment_irradiance_sh_0.xyz *
                  environmentShBasis(0u, normal);
    irradiance += lighting.environment_irradiance_sh_1.xyz *
                  environmentShBasis(1u, normal);
    irradiance += lighting.environment_irradiance_sh_2.xyz *
                  environmentShBasis(2u, normal);
    irradiance += lighting.environment_irradiance_sh_3.xyz *
                  environmentShBasis(3u, normal);
    irradiance += lighting.environment_irradiance_sh_4.xyz *
                  environmentShBasis(4u, normal);
    irradiance += lighting.environment_irradiance_sh_5.xyz *
                  environmentShBasis(5u, normal);
    irradiance += lighting.environment_irradiance_sh_6.xyz *
                  environmentShBasis(6u, normal);
    irradiance += lighting.environment_irradiance_sh_7.xyz *
                  environmentShBasis(7u, normal);
    irradiance += lighting.environment_irradiance_sh_8.xyz *
                  environmentShBasis(8u, normal);
    return max(irradiance, float3(0.0));
}

uint environmentProbeIndex(uint3 coordinate, uint3 dimensions)
{
    return coordinate.x +
           dimensions.x * (coordinate.y + dimensions.y * coordinate.z);
}

bool environmentProbeIsUsable(EnvironmentProbeSummary summary,
                              U_ShadingInfo lighting)
{
    return summary.revisions_state.z == 3u &&
           summary.revisions_state.y == lighting.environment_revision &&
           summary.sample_position_confidence.w > 0.0;
}

float3 evaluateProbeEnvironmentIrradiance(uint probeIndex, float3 normal)
{
    EnvironmentProbeCoefficients coefficients =
        environment_probe_coefficients.data[probeIndex];
    float3 irradiance = float3(0.0);
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        irradiance += coefficients.coefficients[coefficientIndex].xyz *
                      environmentShBasis(coefficientIndex, normal);
    }
    return max(irradiance, float3(0.0));
}

float environmentProbeSignNotZero(float value)
{
    return value >= 0.0 ? 1.0 : -1.0;
}

float2 encodeEnvironmentProbeDirection(float3 direction)
{
    direction /= abs(direction.x) + abs(direction.y) + abs(direction.z);
    float2 encoded = float2(direction.x, direction.z);
    if (direction.y < 0.0)
    {
        float2 unfolded = encoded;
        encoded.x = (1.0 - abs(unfolded.y)) *
                    environmentProbeSignNotZero(unfolded.x);
        encoded.y = (1.0 - abs(unfolded.x)) *
                    environmentProbeSignNotZero(unfolded.y);
    }
    return encoded;
}

float2 environmentProbeDistanceMoments(
    EnvironmentProbeVisibility visibility,
    uint directionIndex,
    float maximumDistance)
{
    uint packed = visibility.packed_distance_moments[directionIndex];
    float2 normalizedMoments = float2(
        packed & 0xffffu,
        packed >> 16u) / 65535.0;
    return normalizedMoments * float2(
        maximumDistance,
        maximumDistance * maximumDistance);
}

float environmentProbeChebyshevVisibility(
    float2 distanceMoments,
    float distanceToSurface,
    float bias)
{
    float biasedDistance = max(0.0, distanceToSurface - bias);
    if (biasedDistance <= distanceMoments.x) return 1.0;

    float variance = abs(
        distanceMoments.y - distanceMoments.x * distanceMoments.x);
    float distancePastMean = biasedDistance - distanceMoments.x;
    float chebyshev = variance /
        max(variance + distancePastMean * distancePastMean, 1.0e-8);
    chebyshev *= chebyshev * chebyshev;
    return max(0.05, chebyshev);
}

float environmentProbeDirectionalVisibility(
    EnvironmentProbeVisibility visibility,
    float3 direction,
    float distanceToSurface,
    float bias,
    float maximumDistance)
{
    float2 encoded = encodeEnvironmentProbeDirection(direction);
    float2 gridPosition = clamp(
        (encoded * 0.5 + 0.5) *
            float(ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE) - 0.5,
        float2(0.0),
        float2(float(ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE - 1u)));
    uint2 base = uint2(floor(gridPosition));
    uint2 next = min(
        base + uint2(1u),
        uint2(ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE - 1u));
    float2 fraction = gridPosition - float2(base);

    uint direction00 = base.x +
        ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE * base.y;
    uint direction10 = next.x +
        ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE * base.y;
    uint direction01 = base.x +
        ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE * next.y;
    uint direction11 = next.x +
        ENVIRONMENT_PROBE_DIRECTION_GRID_SIDE * next.y;
    float2 moments00 = environmentProbeDistanceMoments(
        visibility, direction00, maximumDistance);
    float2 moments10 = environmentProbeDistanceMoments(
        visibility, direction10, maximumDistance);
    float2 moments01 = environmentProbeDistanceMoments(
        visibility, direction01, maximumDistance);
    float2 moments11 = environmentProbeDistanceMoments(
        visibility, direction11, maximumDistance);
    float2 moments = lerp(
        lerp(moments00, moments10, fraction.x),
        lerp(moments01, moments11, fraction.x),
        fraction.y);
    return environmentProbeChebyshevVisibility(
        moments, distanceToSurface, bias);
}

float environmentProbeSurfaceVisibility(
    U_ShadingInfo lighting,
    uint probeIndex,
    float3 probePosition,
    float3 worldPosition)
{
    float3 probeToSurface = worldPosition - probePosition;
    float distanceToSurface = length(probeToSurface);
    float bias = max(lighting.environment_probe_visibility_bias_world, 1.0e-5);
    if (distanceToSurface <= bias) return 1.0;

    float3 direction = probeToSurface / distanceToSurface;
    float3 probeSpacingWorld =
        rcp(lighting.environment_probe_world_to_grid_scale);
    float maximumDistance = length(probeSpacingWorld) * 1.5;
    return environmentProbeDirectionalVisibility(
        environment_probe_visibility.data[probeIndex],
        direction,
        distanceToSurface,
        bias,
        maximumDistance);
}

public float3 sampleEnvironmentIrradiance(U_ShadingInfo lighting,
                                           float3 worldPosition,
                                           float3 surfaceNormal)
{
    float normalLengthSquared = dot(surfaceNormal, surfaceNormal);
    float3 normal = normalLengthSquared > 1.0e-8
        ? surfaceNormal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
    float3 globalIrradiance =
        evaluateGlobalEnvironmentIrradiance(lighting, normal);

    uint3 dimensions = lighting.environment_probe_grid_dimensions;
    if (any(dimensions < uint3(2u)) ||
        any(lighting.environment_probe_world_to_grid_scale <= float3(0.0)))
    {
        return globalIrradiance;
    }

    float3 gridMaximum = float3(dimensions - uint3(1u));
    float3 gridPosition = clamp(
        worldPosition * lighting.environment_probe_world_to_grid_scale,
        float3(0.0), gridMaximum);
    if (lighting.environment_probe_uniform_field != 0u)
    {
        uint3 nearestCoordinate =
            min(uint3(round(gridPosition)), dimensions - uint3(1u));
        uint nearestIndex =
            environmentProbeIndex(nearestCoordinate, dimensions);
        EnvironmentProbeSummary nearestSummary =
            environment_probe_summaries.data[nearestIndex];
        return environmentProbeIsUsable(nearestSummary, lighting)
            ? evaluateProbeEnvironmentIrradiance(nearestIndex, normal)
            : globalIrradiance;
    }

    uint3 base = min(uint3(floor(gridPosition)), dimensions - uint3(2u));
    float3 fraction = gridPosition - float3(base);

    float3 interpolatedCoefficients[9];
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        interpolatedCoefficients[coefficientIndex] = float3(0.0);
    }

    float accumulatedWeight = 0.0;
    bool hasUsableProbe = false;
    bool hasNearestTrustedProbe = false;
    uint nearestTrustedProbeIndex = 0u;
    float nearestTrustedDistanceSquared = 1.0e30;
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                uint3 offset = uint3(x, y, z);
                float3 cornerWeight = lerp(
                    float3(1.0) - fraction, fraction, float3(offset));
                float weight =
                    cornerWeight.x * cornerWeight.y * cornerWeight.z;
                uint probeIndex =
                    environmentProbeIndex(base + offset, dimensions);
                EnvironmentProbeSummary summary =
                    environment_probe_summaries.data[probeIndex];
                if (!environmentProbeIsUsable(summary, lighting)) continue;
                hasUsableProbe = true;

                float3 probePosition =
                    summary.sample_position_confidence.xyz;
                float3 surfaceToProbe = probePosition - worldPosition;
                float probeDistanceSquared = dot(surfaceToProbe, surfaceToProbe);
                float3 surfaceToProbeDirection =
                    probeDistanceSquared > 1.0e-10
                        ? surfaceToProbe * rsqrt(probeDistanceSquared)
                        : normal;
                // A probe on the back side of the surface cannot represent the
                // incident environment for this side of a wall or roof.
                float normalWeight =
                    saturate(dot(normal, surfaceToProbeDirection));
                normalWeight *= normalWeight;
                float visibilityWeight = environmentProbeSurfaceVisibility(
                    lighting,
                    probeIndex,
                    probePosition,
                    worldPosition);
                // Keep the sparse 8x8 moment field continuous here. Cubically
                // crushing small weights reintroduced probe-cell bands at the
                // 16-voxel correctness-test density.
                float trustWeight = normalWeight * visibilityWeight;
                if (trustWeight > 1.0e-5 &&
                    probeDistanceSquared < nearestTrustedDistanceSquared)
                {
                    hasNearestTrustedProbe = true;
                    nearestTrustedProbeIndex = probeIndex;
                    nearestTrustedDistanceSquared = probeDistanceSquared;
                }
                weight *=
                    saturate(summary.sample_position_confidence.w) *
                    trustWeight;
                EnvironmentProbeCoefficients coefficients =
                    environment_probe_coefficients.data[probeIndex];
                [unroll]
                for (uint coefficientIndex = 0u;
                     coefficientIndex < 9u;
                     ++coefficientIndex)
                {
                    interpolatedCoefficients[coefficientIndex] +=
                        coefficients.coefficients[coefficientIndex].xyz *
                        weight;
                }
                accumulatedWeight += weight;
            }
        }
    }

    if (accumulatedWeight <= 1.0e-5)
    {
        if (hasNearestTrustedProbe)
        {
            return evaluateProbeEnvironmentIrradiance(
                nearestTrustedProbeIndex, normal);
        }
        return hasUsableProbe
            ? float3(0.0)
            : globalIrradiance;
    }

    float3 irradiance = float3(0.0);
    float inverseWeight = rcp(accumulatedWeight);
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        irradiance +=
            interpolatedCoefficients[coefficientIndex] *
            inverseWeight *
            environmentShBasis(coefficientIndex, normal);
    }
    return max(irradiance, float3(0.0));
}
