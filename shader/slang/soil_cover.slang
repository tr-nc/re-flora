import voxel_data;
import voxel_types;

// CPU sends only affected cells and the nine live-root membership bits around
// each cell. Exposure stays GPU-owned; no atlas readback or cached moisture.
[[vk::binding(0, 0)]] RWStructuredBuffer<uint4> soil_cover_patches;
[[vk::binding(1, 0)]] [[vk::image_format("r8ui")]] RWTexture3D<uint> chunk_atlas;
struct SoilCoverPush { uint count; uint phase; uint offset; uint reserved; };
[[vk::push_constant]] ConstantBuffer<SoilCoverPush> pc;

bool exposedDirt(int3 p, uint3 size)
{
    if (any(p < int3(0)) || any(p >= int3(size))) return false;
    return voxelTypeFromAtlasData(chunk_atlas[p]) == VOXEL_TYPE_DIRT &&
        (p.y + 1 == int(size.y) || voxelTypeFromAtlasData(chunk_atlas[p + int3(0,1,0)]) == VOXEL_TYPE_EMPTY);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 tid : SV_DispatchThreadID)
{
    if (tid.x >= pc.count) return;
    uint index = pc.offset + tid.x;
    uint4 patch = soil_cover_patches[index];
    if (pc.phase == 0u)
    {
        uint3 size;
        chunk_atlas.GetDimensions(size.x, size.y, size.z);
        uint valid = 0u;
        if (exposedDirt(int3(patch.xyz), size))
        {
            for (uint bit = 0u; bit < 9u; ++bit)
            {
                if ((patch.w & (1u << bit)) != 0u && exposedDirt(
                    int3(patch.xyz) + int3(int(bit % 3u) - 1, 0, int(bit / 3u) - 1), size))
                    valid |= 1u << bit;
            }
        }
        // One voxel under the stem (width = one voxel), plus enclosed one-cell
        // gaps. Opposite roots can bridge an interior gap, never dilate an edge.
        bool covered = (valid & (1u << 4u)) != 0u;
        for (uint bit = 0u; bit < 4u; ++bit)
            covered = covered || ((valid & (1u << bit)) != 0u && (valid & (1u << (8u-bit))) != 0u);
        soil_cover_patches[index].w = covered ? 1u : 0u;
    }
    else
    {
        // Separate dispatch: no invocation reads neighbors while any invocation
        // writes the atlas. Destinations are unique, so no r8 image write races.
        uint data = chunk_atlas[patch.xyz];
        if (voxelTypeFromAtlasData(data) == VOXEL_TYPE_DIRT)
            chunk_atlas[patch.xyz] = (data & ~VOXEL_GRASS_COVER_BIT) |
                (patch.w != 0u ? VOXEL_GRASS_COVER_BIT : 0u);
    }
}
