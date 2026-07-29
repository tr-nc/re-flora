import contree_node;
import marching_result;
import scene_marching;
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
};

struct EnvironmentProbeDirection
{
    float4 direction;
    float4 irradiance_coefficients[9];
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

struct B_EnvironmentProbeDirections
{
    EnvironmentProbeDirection data[];
};

[[vk::binding(2, 0)]]
ConstantBuffer<U_ShadingInfo> shading_info;

[[vk::binding(7, 0)]]
StructuredBuffer<ContreeNode> contree_node_data;

[[vk::binding(8, 0)]]
StructuredBuffer<uint> contree_leaf_data;

[[vk::binding(9, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

[[vk::binding(18, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeCoefficients, Std430DataLayout>
    environment_probe_coefficients;

[[vk::binding(19, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

[[vk::binding(20, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeVisibility, Std430DataLayout>
    environment_probe_visibility;

[[vk::binding(21, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeDirections, Std430DataLayout>
    environment_probe_directions;

struct EnvironmentProbeUpdatePushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint environment_revision;
    uint trace_visibility;
    uint3 priority_grid_min;
    uint priority_side;
};

[[vk::push_constant]]
ConstantBuffer<EnvironmentProbeUpdatePushConstants> pc;

static const uint PROBE_STATE_VALID = 3u;
static const uint PROBE_STATE_DIRTY = 4u;
static const uint PROBE_DIRECTION_COUNT = 64u;
static const uint PROBE_MISS_DISTANCE = 65535u;
static const uint PROBE_AXIAL_MISS_DISTANCE = 1023u;
static const float PROBE_MAX_HIT_DISTANCE_WORLD = 4.0;
static const float3 PROBE_AXIAL_DIRECTIONS[6] =
{
    float3(1.0, 0.0, 0.0),
    float3(-1.0, 0.0, 0.0),
    float3(0.0, 1.0, 0.0),
    float3(0.0, -1.0, 0.0),
    float3(0.0, 0.0, 1.0),
    float3(0.0, 0.0, -1.0),
};

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

float environmentIrradianceBandFactor(uint coefficientIndex)
{
    if (coefficientIndex == 0u) return 3.14159265359;
    if (coefficientIndex <= 3u) return 2.09439510239;
    return 0.78539816339;
}

float3 globalIrradianceCoefficient(uint coefficientIndex)
{
    switch (coefficientIndex)
    {
        case 0u: return shading_info.environment_irradiance_sh_0.xyz;
        case 1u: return shading_info.environment_irradiance_sh_1.xyz;
        case 2u: return shading_info.environment_irradiance_sh_2.xyz;
        case 3u: return shading_info.environment_irradiance_sh_3.xyz;
        case 4u: return shading_info.environment_irradiance_sh_4.xyz;
        case 5u: return shading_info.environment_irradiance_sh_5.xyz;
        case 6u: return shading_info.environment_irradiance_sh_6.xyz;
        case 7u: return shading_info.environment_irradiance_sh_7.xyz;
        default: return shading_info.environment_irradiance_sh_8.xyz;
    }
}

float3 reconstructedEnvironmentRadiance(float3 direction)
{
    float3 radiance = float3(0.0);
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        radiance +=
            globalIrradianceCoefficient(coefficientIndex) /
            environmentIrradianceBandFactor(coefficientIndex) *
            environmentShBasis(coefficientIndex, direction);
    }
    return max(radiance, float3(0.0));
}

uint quantizeHitDistance(float distanceWorld)
{
    float normalizedDistance =
        saturate(distanceWorld / PROBE_MAX_HIT_DISTANCE_WORLD);
    return max(1u, min(65534u, uint(round(normalizedDistance * 65534.0))));
}

uint quantizeAxialHitDistance(float distanceWorld)
{
    float normalizedDistance =
        saturate(distanceWorld / PROBE_MAX_HIT_DISTANCE_WORLD);
    return max(1u, min(1022u, uint(round(normalizedDistance * 1022.0))));
}

uint packedDistance(EnvironmentProbeVisibility visibility, uint directionIndex)
{
    uint packed = visibility.packed_hit_distances[directionIndex / 2u];
    return (directionIndex & 1u) == 0u
        ? packed & 0xffffu
        : packed >> 16u;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint localProbeIndex = dispatchThreadId.x;
    if (localProbeIndex >= pc.probe_count) return;

    uint3 dimensions = shading_info.environment_probe_grid_dimensions;
    uint totalProbeCount = dimensions.x * dimensions.y * dimensions.z;
    uint probeIndex;
    if (pc.priority_side > 0u)
    {
        uint priorityPlane = pc.priority_side * pc.priority_side;
        uint localZ = localProbeIndex / priorityPlane;
        uint localRemainder = localProbeIndex % priorityPlane;
        uint localY = localRemainder / pc.priority_side;
        uint localX = localRemainder % pc.priority_side;
        uint3 coordinate =
            pc.priority_grid_min + uint3(localX, localY, localZ);
        if (any(coordinate >= dimensions)) return;
        probeIndex = coordinate.x +
                     dimensions.x *
                         (coordinate.y + dimensions.y * coordinate.z);
    }
    else
    {
        probeIndex = pc.first_probe_index + localProbeIndex;
    }
    if (probeIndex >= totalProbeCount) return;

    EnvironmentProbeSummary summary =
        environment_probe_summaries.data[probeIndex];
    bool tracing = pc.trace_visibility != 0u;
    if ((tracing && summary.revisions_state.z != PROBE_STATE_DIRTY) ||
        (!tracing && summary.revisions_state.z != PROBE_STATE_VALID))
    {
        return;
    }

    EnvironmentProbeVisibility visibility =
        environment_probe_visibility.data[probeIndex];
    float3 capturedVisible[9];
    float3 capturedAllVisible[9];
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        capturedVisible[coefficientIndex] = float3(0.0);
        capturedAllVisible[coefficientIndex] = float3(0.0);
    }

    uint skyMissCount = 0u;
    uint skyDirectionCount = 0u;
    for (uint pairIndex = 0u; pairIndex < 32u; ++pairIndex)
    {
        uint packed = 0u;
        [unroll]
        for (uint pairMember = 0u; pairMember < 2u; ++pairMember)
        {
            uint directionIndex = pairIndex * 2u + pairMember;
            EnvironmentProbeDirection directionRecord =
                environment_probe_directions.data[directionIndex];
            float3 direction = normalize(directionRecord.direction.xyz);
            bool environmentMiss;
            uint hitDistance;
            if (tracing)
            {
                MarchingResult result = marchScene(
                    contree_node_data, contree_leaf_data, scene_tex,
                    summary.sample_position_confidence.xyz, direction,
                    1.0 / direction, localInvocationIndex);
                environmentMiss = !result.is_hit;
                hitDistance = environmentMiss
                    ? PROBE_MISS_DISTANCE
                    : quantizeHitDistance(result.distance);
            }
            else
            {
                hitDistance = packedDistance(visibility, directionIndex);
                environmentMiss = hitDistance == PROBE_MISS_DISTANCE;
            }

            packed |= hitDistance << (pairMember * 16u);
            bool samplesSky = direction.y >= -1.0e-6;
            skyDirectionCount += samplesSky ? 1u : 0u;
            skyMissCount += samplesSky && environmentMiss ? 1u : 0u;

            float3 radiance = reconstructedEnvironmentRadiance(direction);
            [unroll]
            for (uint coefficientIndex = 0u;
                 coefficientIndex < 9u;
                 ++coefficientIndex)
            {
                float3 contribution =
                    radiance *
                    directionRecord
                        .irradiance_coefficients[coefficientIndex].x;
                capturedAllVisible[coefficientIndex] += contribution;
                if (environmentMiss)
                {
                    capturedVisible[coefficientIndex] += contribution;
                }
            }
        }
        if (tracing)
        {
            visibility.packed_hit_distances[pairIndex] = packed;
        }
    }

    if (tracing)
    {
        environment_probe_visibility.data[probeIndex] = visibility;
        uint2 packedAxialDistances = uint2(0u);
        [unroll]
        for (uint axialDirectionIndex = 0u;
             axialDirectionIndex < 6u;
             ++axialDirectionIndex)
        {
            float3 axialDirection =
                PROBE_AXIAL_DIRECTIONS[axialDirectionIndex];
            MarchingResult axialResult = marchScene(
                contree_node_data, contree_leaf_data, scene_tex,
                summary.sample_position_confidence.xyz, axialDirection,
                1.0 / axialDirection, localInvocationIndex);
            uint axialHitDistance = axialResult.is_hit
                ? quantizeAxialHitDistance(axialResult.distance)
                : PROBE_AXIAL_MISS_DISTANCE;
            uint packedIndex = axialDirectionIndex / 3u;
            uint packedMember = axialDirectionIndex % 3u;
            packedAxialDistances[packedIndex] |=
                axialHitDistance << (packedMember * 10u);
        }
        summary.update_info.x = packedAxialDistances.x;
        summary.update_info.y = packedAxialDistances.y;
    }

    float skyVisibility = skyDirectionCount > 0u
        ? float(skyMissCount) / float(skyDirectionCount)
        : 0.0;
    EnvironmentProbeCoefficients coefficients;
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        float3 coefficient =
            capturedVisible[coefficientIndex] +
            skyVisibility *
                (globalIrradianceCoefficient(coefficientIndex) -
                 capturedAllVisible[coefficientIndex]);
        coefficients.coefficients[coefficientIndex] =
            float4(coefficient, 0.0);
    }
    environment_probe_coefficients.data[probeIndex] = coefficients;

    summary.original_position_visibility.w = skyVisibility;
    summary.sample_position_confidence.w = 1.0;
    summary.revisions_state.y = pc.environment_revision;
    summary.revisions_state.z = PROBE_STATE_VALID;
    environment_probe_summaries.data[probeIndex] = summary;
}
