import chunk_writer_types;
import voxel_data;
import voxel_types;

[[vk::binding(0, 0)]]
ConstantBuffer<U_ChunkSolidSampleInfo> chunk_solid_sample_info;

[[vk::binding(1, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(2, 0)]]
GLSLShaderStorageBuffer<B_ChunkSolidSamples, Std430DataLayout>
    chunk_solid_samples;

uint sampleIndex(uint3 position, uint3 dimension)
{
    return (position.z * dimension.y + position.y) * dimension.x + position.x;
}

uint loadVoxelType(int3 position)
{
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    if (any(position < int3(0)) ||
        any(position >= int3(width, height, depth)))
    {
        return VOXEL_TYPE_EMPTY;
    }
    return voxelTypeFromAtlasData(chunk_atlas[position]);
}

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 sampleId = dispatchThreadId;
    uint3 sampleDimension = chunk_solid_sample_info.sample_dim;
    if (any(sampleId >= sampleDimension))
    {
        return;
    }

    uint3 sourceDimension = chunk_solid_sample_info.atlas_dim;
    uint3 sourceVoxel = min(
        ((sampleId * uint3(2u) + uint3(1u)) * sourceDimension) /
            (sampleDimension * uint3(2u)),
        sourceDimension - uint3(1u));
    uint3 atlasVoxel = chunk_solid_sample_info.atlas_offset + sourceVoxel;

    uint voxelType = loadVoxelType(int3(atlasVoxel));
    chunk_solid_samples.solid[sampleIndex(sampleId, sampleDimension)] =
        voxelType == VOXEL_TYPE_EMPTY ? 0u : 1u;
}
