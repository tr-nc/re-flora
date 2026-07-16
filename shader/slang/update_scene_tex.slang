struct U_SceneTexUpdateInfo
{
    uint3 chunk_idx;
    uint node_offset_for_chunk;
    uint leaf_offset_for_chunk;
    uint is_valid;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_SceneTexUpdateInfo> scene_tex_update_info;

[[vk::binding(1, 0)]]
[[vk::image_format("rg32ui")]]
WTexture3D<uint2> scene_tex;

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    if (scene_tex_update_info.is_valid == 0u)
    {
        scene_tex.Store(scene_tex_update_info.chunk_idx, uint2(0u));
        return;
    }

    scene_tex.Store(
        scene_tex_update_info.chunk_idx,
        uint2(scene_tex_update_info.node_offset_for_chunk,
              scene_tex_update_info.leaf_offset_for_chunk) + 1u);
}
