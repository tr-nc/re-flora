#ifndef RE_FLORA_GLASS_TRANSPORT
#define RE_FLORA_GLASS_TRANSPORT 0
#endif

import contree_node;
import local_lighting;
import scene_marching;

public static const uint LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_ABI_VERSION = 3u;
public static const uint LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_SPECIFIC = 1u;
public static const uint LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_AGGREGATE = 2u;

public struct U_LocalLightVisibilityDiagnosticInfo
{
    public uint abi_version;
    public uint request_serial;
    public uint geometry_revision;
    public uint source_revision_low;
    public uint source_revision_high;
    public uint light_id_slot;
    public uint light_id_generation;
    public uint mode;
    public float4 receiver_position_and_origin_offset;
    public float4 receiver_normal_and_reserved;
};

public struct B_LocalLightVisibilityDiagnosticResult
{
    public uint abi_version;
    public uint request_serial;
    public uint geometry_revision;
    public uint mode;
    public uint identity_matches;
    public uint selected_light_index;
    public uint source_revision_low;
    public uint source_revision_high;
    public uint light_id_slot;
    public uint light_id_generation;
    public uint candidates;
    public uint visible;
    public uint occluded;
    public uint irradiance_luma_q8;
    public float4 irradiance;
    public float4 receiver_position_and_origin_offset;
    public float4 receiver_normal_and_reserved;
};

ConstantBuffer<U_LocalLightVisibilityDiagnosticInfo>
    local_light_visibility_diagnostic_info;
GLSLShaderStorageBuffer<B_LocalLightVisibilityDiagnosticResult, Std430DataLayout>
    local_light_visibility_diagnostic_result;
ConstantBuffer<U_LocalLightInfo> local_light_info;
StructuredBuffer<U_LightGpu> local_lights;
StructuredBuffer<ContreeNode> contree_node_data;
StructuredBuffer<uint> contree_leaf_data;
readonly RWTexture3D<uint2> scene_tex;
#if RE_FLORA_GLASS_TRANSPORT
readonly RWTexture3D<uint> chunk_atlas;
#endif

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    U_LocalLightVisibilityDiagnosticInfo info =
        local_light_visibility_diagnostic_info;
    B_LocalLightVisibilityDiagnosticResult result;
    result.abi_version = LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_ABI_VERSION;
    result.request_serial = info.request_serial;
    result.geometry_revision = info.geometry_revision;
    result.mode = info.mode;
    result.selected_light_index = 0xffffffffu;
    result.source_revision_low = info.source_revision_low;
    result.source_revision_high = info.source_revision_high;
    result.light_id_slot = info.light_id_slot;
    result.light_id_generation = info.light_id_generation;
    result.candidates = 0u;
    result.visible = 0u;
    result.occluded = 0u;
    result.irradiance_luma_q8 = 0u;
    result.irradiance = float4(0.0);
    result.receiver_position_and_origin_offset =
        info.receiver_position_and_origin_offset;
    result.receiver_normal_and_reserved = info.receiver_normal_and_reserved;

    uint requestedLightIndex = 0u;
    bool foundRequestedLight = findLocalLightIndexByStableId(
        local_light_info, local_lights, info.light_id_slot,
        info.light_id_generation, requestedLightIndex);
    bool snapshotMatches =
        info.abi_version == LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_ABI_VERSION &&
        local_light_info.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION &&
        local_light_info.source_revision_low == info.source_revision_low &&
        local_light_info.source_revision_high == info.source_revision_high;
    bool specificMode =
        info.mode == LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_SPECIFIC;
    bool aggregateMode =
        info.mode == LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_AGGREGATE;
    bool identityMatches = snapshotMatches &&
        ((specificMode && foundRequestedLight) || aggregateMode);
    result.identity_matches = identityMatches ? 1u : 0u;
    result.selected_light_index = identityMatches && specificMode
        ? requestedLightIndex : 0xffffffffu;
    if (identityMatches && specificMode)
    {
#if RE_FLORA_GLASS_TRANSPORT
        LocalLightVoxelEvaluation evaluation =
            evaluateVoxelGlassVisibleLocalLightAtIndex(
                local_light_info, local_lights, contree_node_data,
                contree_leaf_data, scene_tex, chunk_atlas,
                info.receiver_position_and_origin_offset.xyz,
                info.receiver_normal_and_reserved.xyz,
                info.receiver_position_and_origin_offset.w, 0u,
                requestedLightIndex);
#else
        LocalLightVoxelEvaluation evaluation = evaluateVoxelVisibleLocalLightAtIndex(
            local_light_info, local_lights, contree_node_data, contree_leaf_data,
            scene_tex,
            info.receiver_position_and_origin_offset.xyz,
            info.receiver_normal_and_reserved.xyz,
            info.receiver_position_and_origin_offset.w, 0u, requestedLightIndex);
#endif
        result.candidates = evaluation.candidates;
        result.visible = evaluation.visible;
        result.occluded = evaluation.occluded;
        result.irradiance_luma_q8 = evaluation.irradiance_luma_q8;
        result.irradiance = float4(evaluation.irradiance, 0.0);
    }
    else if (identityMatches && aggregateMode)
    {
#if RE_FLORA_GLASS_TRANSPORT
        LocalLightVoxelEvaluation evaluation = evaluateVoxelGlassVisibleLocalLight(
            local_light_info, local_lights, contree_node_data, contree_leaf_data,
            scene_tex, chunk_atlas,
            info.receiver_position_and_origin_offset.xyz,
            info.receiver_normal_and_reserved.xyz,
            info.receiver_position_and_origin_offset.w, 0u);
#else
        LocalLightVoxelEvaluation evaluation = evaluateVoxelVisibleLocalLight(
            local_light_info, local_lights, contree_node_data, contree_leaf_data,
            scene_tex,
            info.receiver_position_and_origin_offset.xyz,
            info.receiver_normal_and_reserved.xyz,
            info.receiver_position_and_origin_offset.w, 0u);
#endif
        result.candidates = evaluation.candidates;
        result.visible = evaluation.visible;
        result.occluded = evaluation.occluded;
        result.irradiance_luma_q8 = evaluation.irradiance_luma_q8;
        result.irradiance = float4(evaluation.irradiance, 0.0);
    }
    local_light_visibility_diagnostic_result.abi_version = result.abi_version;
    local_light_visibility_diagnostic_result.request_serial = result.request_serial;
    local_light_visibility_diagnostic_result.geometry_revision =
        result.geometry_revision;
    local_light_visibility_diagnostic_result.mode = result.mode;
    local_light_visibility_diagnostic_result.identity_matches =
        result.identity_matches;
    local_light_visibility_diagnostic_result.selected_light_index =
        result.selected_light_index;
    local_light_visibility_diagnostic_result.source_revision_low =
        result.source_revision_low;
    local_light_visibility_diagnostic_result.source_revision_high =
        result.source_revision_high;
    local_light_visibility_diagnostic_result.light_id_slot = result.light_id_slot;
    local_light_visibility_diagnostic_result.light_id_generation =
        result.light_id_generation;
    local_light_visibility_diagnostic_result.candidates = result.candidates;
    local_light_visibility_diagnostic_result.visible = result.visible;
    local_light_visibility_diagnostic_result.occluded = result.occluded;
    local_light_visibility_diagnostic_result.irradiance_luma_q8 =
        result.irradiance_luma_q8;
    local_light_visibility_diagnostic_result.irradiance = result.irradiance;
    local_light_visibility_diagnostic_result.receiver_position_and_origin_offset =
        result.receiver_position_and_origin_offset;
    local_light_visibility_diagnostic_result.receiver_normal_and_reserved =
        result.receiver_normal_and_reserved;
}
