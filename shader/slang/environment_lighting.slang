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

struct B_EnvironmentProbeSummaries
{
    EnvironmentProbeSummary data[];
};

struct B_EnvironmentProbeCoefficients
{
    EnvironmentProbeCoefficients data[];
};

[[vk::binding(18, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeCoefficients, Std430DataLayout>
    environment_probe_coefficients;

[[vk::binding(19, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

static const uint ENVIRONMENT_PROBE_AXIAL_MISS_DISTANCE = 1023u;
static const float ENVIRONMENT_PROBE_MAX_HIT_DISTANCE_WORLD = 4.0;

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

uint environmentProbeAxialHitDistance(
    EnvironmentProbeSummary summary,
    uint directionIndex)
{
    uint packed = directionIndex < 3u
        ? summary.update_info.x
        : summary.update_info.y;
    uint packedIndex = directionIndex < 3u
        ? directionIndex
        : directionIndex - 3u;
    return (packed >> (packedIndex * 10u)) & 0x3ffu;
}

float environmentProbeAxialHitVisibility(
    uint quantizedDistance,
    float distanceToSurface,
    float bias)
{
    if (quantizedDistance == ENVIRONMENT_PROBE_AXIAL_MISS_DISTANCE)
        return 1.0;

    float hitDistance =
        float(quantizedDistance) /
        float(ENVIRONMENT_PROBE_AXIAL_MISS_DISTANCE - 1u) *
        ENVIRONMENT_PROBE_MAX_HIT_DISTANCE_WORLD;
    float distancePastHit = distanceToSurface - hitDistance;
    return 1.0 - smoothstep(bias, bias * 3.0, distancePastHit);
}

float environmentProbeSurfaceVisibility(
    U_ShadingInfo lighting,
    EnvironmentProbeSummary summary,
    float3 probePosition,
    float3 worldPosition)
{
    float3 probeToSurface = worldPosition - probePosition;
    float distanceToSurface = length(probeToSurface);
    float bias = max(lighting.environment_probe_visibility_bias_world, 1.0e-5);
    if (distanceToSurface <= bias) return 1.0;

    float3 direction = probeToSurface / distanceToSurface;
    float3 axisWeights = abs(direction);
    axisWeights *= axisWeights;
    uint xDirectionIndex = direction.x >= 0.0 ? 0u : 1u;
    uint yDirectionIndex = direction.y >= 0.0 ? 2u : 3u;
    uint zDirectionIndex = direction.z >= 0.0 ? 4u : 5u;
    float3 visibility = float3(
        environmentProbeAxialHitVisibility(
            environmentProbeAxialHitDistance(
                summary, xDirectionIndex),
            distanceToSurface, bias),
        environmentProbeAxialHitVisibility(
            environmentProbeAxialHitDistance(
                summary, yDirectionIndex),
            distanceToSurface, bias),
        environmentProbeAxialHitVisibility(
            environmentProbeAxialHitDistance(
                summary, zDirectionIndex),
            distanceToSurface, bias));
    return dot(visibility, axisWeights) /
           max(axisWeights.x + axisWeights.y + axisWeights.z, 1.0e-5);
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
    bool hasNearestUsableProbe = false;
    uint nearestUsableProbeIndex = 0u;
    float nearestUsableDistanceSquared = 1.0e30;
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

                float3 probePosition =
                    summary.sample_position_confidence.xyz;
                float3 surfaceToProbe = probePosition - worldPosition;
                float probeDistanceSquared = dot(surfaceToProbe, surfaceToProbe);
                if (probeDistanceSquared < nearestUsableDistanceSquared)
                {
                    hasNearestUsableProbe = true;
                    nearestUsableProbeIndex = probeIndex;
                    nearestUsableDistanceSquared = probeDistanceSquared;
                }

                float3 surfaceToProbeDirection =
                    probeDistanceSquared > 1.0e-10
                        ? surfaceToProbe * rsqrt(probeDistanceSquared)
                        : normal;
                float normalWeight = saturate(
                    dot(normal, surfaceToProbeDirection) * 0.5 + 0.5);
                normalWeight *= normalWeight;
                float visibilityWeight = environmentProbeSurfaceVisibility(
                    lighting,
                    summary,
                    probePosition,
                    worldPosition);
                weight *=
                    saturate(summary.sample_position_confidence.w) *
                    normalWeight *
                    visibilityWeight;
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
        return hasNearestUsableProbe
            ? evaluateProbeEnvironmentIrradiance(
                  nearestUsableProbeIndex, normal)
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
