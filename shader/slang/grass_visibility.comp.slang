import flora_types;
import grass_visibility_bounds;
import flora_motion;
import tracer_types;
import vegetation_response_sample;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(6, 0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(7, 0)]] ConstantBuffer<U_WindVolumeInfo> wind_volume_info;
[[vk::binding(8, 0)]] Sampler3D wind_volume_tex;

struct PushConstantGrassVisibility {
    uint3 chunk_world_offset;
    uint species;
    uint instance_count;
    uint word_offset;
    uint enabled;
    uint bands;
};
[[vk::push_constant]] ConstantBuffer<PushConstantGrassVisibility> visibility_pc;
struct B_GrassVisibilitySource { Instance data[]; };
[[vk::binding(0, 1)]] readonly GLSLShaderStorageBuffer<B_GrassVisibilitySource, Std430DataLayout> flora_instances;
[[vk::binding(10, 1)]] RWStructuredBuffer<uint> grass_visible_instances;

groupshared uint group_count;
groupshared uint group_base;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID, uint lane : SV_GroupIndex) {
    uint source = id.x;
    uint base = visibility_pc.word_offset;
    if (visibility_pc.enabled == 0u) {
        if (source >= visibility_pc.instance_count) return;
        // Diagnostic reference preserves original order without a CPU readback.
        grass_visible_instances[base + 5u + source] = source;
        if (source == 0u) grass_visible_instances[base + 1u] = visibility_pc.instance_count;
        return;
    }
    if (lane == 0u) group_count = 0u;
    bool keep = false;
    if (source < visibility_pc.instance_count) {
        uint species = visibility_pc.species;
        Instance instance = flora_instances.data[species * MAX_FLORA_INSTANCES_PER_SPECIES + source];
        uint3 root = surfaceFloraStemWorldPosition(instance.packed_local_pos,
                                                  visibility_pc.chunk_world_offset);
        uint seed = instanceSeed(root);
        float3 center = float3(root) / 256.0;
        // Use the current published response, including inertia and local flutter;
        // never assume GUI wind strength bounds an already moving plant.
        float3 wind = vegetationResponseEnabled()
            ? sampleVegetationResponse(root, seed, species, source, 0u)
            : sampleWindVolume(wind_volume_info, wind_volume_tex, center, seed);
        keep = grassBoundsVisible(camera_info.view_proj_mat, center,
            grassVisibilityExtent(visibility_pc.bands, wind, gui_input, flora_growth_info));
    }
    // One global reservation per workgroup, not one globally contended atomic
    // per plant. All lanes, including the partial tail, participate in barriers.
    // No optional subgroup-size/ballot capabilities are needed.
    GroupMemoryBarrierWithGroupSync();
    uint local = 0u;
    if (keep) InterlockedAdd(group_count, 1u, local);
    GroupMemoryBarrierWithGroupSync();
    if (lane == 0u && group_count > 0u)
        InterlockedAdd(grass_visible_instances[base + 1u], group_count, group_base);
    GroupMemoryBarrierWithGroupSync();
    if (keep) grass_visible_instances[base + 5u + group_base + local] = source;
}
