struct EnvironmentProbeSummary
{
    float4 original_position_visibility;
    float4 sample_position_confidence;
    uint4 revisions_state;
    uint4 update_info;
};

struct B_EnvironmentProbeSummaries
{
    EnvironmentProbeSummary data[];
};

struct EnvironmentProbeStateCounts
{
    uint counts[7];
    uint padding;
};

struct B_EnvironmentProbeStats
{
    EnvironmentProbeStateCounts data[];
};

[[vk::binding(19, 0)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

[[vk::binding(22, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeStats, Std430DataLayout>
    environment_probe_stats;

struct EnvironmentProbeStatsPushConstants
{
    uint probe_count;
    uint3 padding;
};

[[vk::push_constant]]
ConstantBuffer<EnvironmentProbeStatsPushConstants> pc;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint probeIndex = dispatchThreadId.x;
    if (probeIndex >= pc.probe_count) return;

    uint state = environment_probe_summaries.data[probeIndex].revisions_state.z;
    if (state < 7u)
    {
        InterlockedAdd(environment_probe_stats.data[0].counts[state], 1u);
    }
}
