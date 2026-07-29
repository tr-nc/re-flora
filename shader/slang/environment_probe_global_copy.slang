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

static const uint PROBE_PACKED_AXIAL_MISS_DISTANCES = 0x3fffffffu;

[[vk::binding(2, 0)]]
ConstantBuffer<U_ShadingInfo> shading_info;

[[vk::binding(18, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeCoefficients, Std430DataLayout>
    environment_probe_coefficients;

[[vk::binding(19, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 dimensions = shading_info.environment_probe_grid_dimensions;
    uint probeCount = dimensions.x * dimensions.y * dimensions.z;
    uint probeIndex = dispatchThreadId.x;
    if (probeIndex >= probeCount) return;

    EnvironmentProbeCoefficients coefficients;
    coefficients.coefficients[0] = shading_info.environment_irradiance_sh_0;
    coefficients.coefficients[1] = shading_info.environment_irradiance_sh_1;
    coefficients.coefficients[2] = shading_info.environment_irradiance_sh_2;
    coefficients.coefficients[3] = shading_info.environment_irradiance_sh_3;
    coefficients.coefficients[4] = shading_info.environment_irradiance_sh_4;
    coefficients.coefficients[5] = shading_info.environment_irradiance_sh_5;
    coefficients.coefficients[6] = shading_info.environment_irradiance_sh_6;
    coefficients.coefficients[7] = shading_info.environment_irradiance_sh_7;
    coefficients.coefficients[8] = shading_info.environment_irradiance_sh_8;
    environment_probe_coefficients.data[probeIndex] = coefficients;

    EnvironmentProbeSummary summary =
        environment_probe_summaries.data[probeIndex];
    bool placementClassified = summary.update_info.z != 0u;
    if (!placementClassified)
    {
        summary.original_position_visibility.w = 1.0;
        summary.sample_position_confidence =
            float4(summary.original_position_visibility.xyz, 1.0);
        summary.revisions_state =
            uint4(0u, shading_info.environment_revision, 3u, probeIndex);
        summary.update_info =
            uint4(PROBE_PACKED_AXIAL_MISS_DISTANCES,
                  PROBE_PACKED_AXIAL_MISS_DISTANCES, 0u, 0u);
    }
    else
    {
        summary.revisions_state.y = shading_info.environment_revision;
    }
    environment_probe_summaries.data[probeIndex] = summary;
}
