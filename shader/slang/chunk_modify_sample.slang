import chunk_writer_types;
import hash;
import voxel_types;

struct ChunkModifySamplePushConstants
{
    uint edit_seed;
};

[[vk::push_constant]]
ConstantBuffer<ChunkModifySamplePushConstants> pc;

[[vk::binding(6, 0)]]
readonly GLSLShaderStorageBuffer<B_EditStats, Std430DataLayout> edit_stats;

[[vk::binding(7, 0)]]
readonly GLSLShaderStorageBuffer<B_EditRemovalCandidates, Std430DataLayout>
    edit_removal_candidates;

[[vk::binding(8, 0)]]
GLSLShaderStorageBuffer<B_EditRemovalSample, Std430DataLayout>
    edit_removal_sample;

[shader("compute")]
[numthreads(EDIT_REMOVAL_SAMPLE_COUNT, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint sampleIndex = dispatchThreadId.x;
    uint candidateCount = edit_stats.added_counts[VOXEL_TYPE_EMPTY];
    uint sampleCount = min(candidateCount, EDIT_REMOVAL_SAMPLE_COUNT);

    if (sampleIndex == 0u)
    {
        edit_removal_sample.sample_count = sampleCount;
    }

    if (sampleIndex >= EDIT_REMOVAL_SAMPLE_COUNT)
    {
        return;
    }
    if (sampleIndex >= sampleCount)
    {
        edit_removal_sample.positions[sampleIndex] = float4(0.0);
        return;
    }

    uint candidateIndex = murmurHash12(
        uint2(pc.edit_seed, sampleIndex)) % candidateCount;
    edit_removal_sample.positions[sampleIndex] =
        edit_removal_candidates.positions[candidateIndex];
}
