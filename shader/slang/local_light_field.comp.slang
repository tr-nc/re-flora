#ifndef RE_FLORA_GLASS_TRANSPORT
#define RE_FLORA_GLASS_TRANSPORT 0
#endif

import contree_node;
import ddgi_types;
import ddgi_octahedral;
import tracer_types;
import local_lighting;
import local_emitter;
import local_light_field_policy;
import local_light_math;
import marching_result;
import scene_marching;
#if RE_FLORA_GLASS_TRANSPORT
import scene_query;
#endif

[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(42, 0)]] ConstantBuffer<U_LocalLightInfo> local_light_info;
[[vk::binding(43, 0)]] StructuredBuffer<U_LightGpu> local_lights;
[[vk::binding(44, 0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(45, 0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(46, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;
#if RE_FLORA_GLASS_TRANSPORT
[[vk::binding(15, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;
#endif

// Frame-owned per-source transmission, never bound to recursive DDGI transport.
[[vk::binding(0, 1)]] RWStructuredBuffer<uint2> local_light_field;
[[vk::binding(2, 1)]] RWStructuredBuffer<uint> local_light_field_audit;
[[vk::binding(1, 1)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout> ddgi_probe_metadata;

struct LocalLightFieldPushConstants
{
    uint first_probe;
    uint probe_count;
    uint light_count;
    uint audit;
};
[[vk::push_constant]] ConstantBuffer<LocalLightFieldPushConstants> pc;

groupshared float4 incident[LOCAL_EMITTER_FIXED_SAMPLE_COUNT];
groupshared float4 directions[LOCAL_EMITTER_FIXED_SAMPLE_COUNT];

float3 connectionTransmission(float3 origin, float3 direction, float distance, uint lane)
{
    if (distance <= 1.0e-6) return float3(1.0);
#if RE_FLORA_GLASS_TRANSPORT
    SceneStraightTransport transport = traceVoxelStraightTransport(
        contree_node_data, contree_leaf_data, scene_tex, chunk_atlas,
        origin, direction, distance,
        (local_light_info.flags & LOCAL_LIGHT_FLAG_LEGACY_SAND_GLASS) != 0u, lane);
    return transport.exhausted != 0u || transport.terminal.kind == SCENE_QUERY_EVENT_OPAQUE
        ? float3(0.0) : transport.transmittance;
#else
    MarchingResult hit = marchScene(contree_node_data, contree_leaf_data, scene_tex,
        origin, direction, 1.0 / direction, lane);
    return !hit.is_hit || hit.distance >= distance ? float3(1.0) : float3(0.0);
#endif
}

[shader("compute")]
[numthreads(128, 1, 1)]
void main(uint3 groupId : SV_GroupID, uint lane : SV_GroupIndex)
{
    uint probe = pc.first_probe + groupId.x;
    if (probe >= pc.probe_count) return;
    if (probe == 0u && lane == 0u)
    {
        local_light_field[0] = uint2(LOCAL_LIGHT_FIELD_ABI, pc.probe_count);
        local_light_field[1] = uint2(pc.light_count,
            LOCAL_LIGHT_FIELD_CONNECTIONS | (pc.audit != 0u ? (1u << 16u) : 0u));
    }
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probe];
    bool valid = metadata.state_and_reserved.x == DDGI_PROBE_STATE_VALID;
    float3 position = metadata.actual_position_and_clearance.xyz;
    float supportMargin = length(rcp(shading_info.environment_probe_world_to_grid_scale)) +
        length(position - metadata.nominal_position_and_min_clearance.xyz);
    float3 normal = octDecode((float2(lane % 8u, lane / 8u) + 0.5) / 4.0 - 1.0);
    float3 response = float3(0.0);
    uint count = min(localLightCount(local_light_info), pc.light_count);
    for (uint lightIndex = 0u; lightIndex < count; ++lightIndex)
    {
        U_LightGpu light = local_lights[lightIndex];
        bool finiteEmitter = light.shape_params.w > 0.5;
        uint taps = finiteEmitter ? LOCAL_EMITTER_FIXED_SAMPLE_COUNT : 1u;
        if (lane < LOCAL_EMITTER_FIXED_SAMPLE_COUNT)
        {
            uint2 packed = uint2(0u);
            float3 energy = float3(0.0);
            float3 direction = float3(0.0);
            bool supported = valid && light.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION &&
                light.kind == LOCAL_LIGHT_KIND_POINT && light.intensity > 0.0 &&
                length(position - light.position) <= light.range + supportMargin;
            if (supported && lane < taps)
            {
                float distance;
                if (finiteEmitter)
                {
                    LocalEmitterSample surface = localEmitterFixedSurfaceSample(
                        light.position, light.shape_params.xyz, lane);
                    float3 segment = surface.position - position;
                    direction = segment / max(length(segment), 1.0e-12);
                    distance = max(localEmitterConnectionDistance(position, light.position,
                        light.shape_params.xyz, surface.position) - 1.0e-5, 1.0e-6);
                    LocalEmitterSample sample = sampleLocalEmitterFixedIncident(
                        light.position, light.shape_params.xyz, light.range, position, lane);
                    energy = light.color * light.intensity * sample.irradiance_per_intensity;
                }
                else
                {
                    float3 segment = light.position - position;
                    float length = sqrt(dot(segment, segment));
                    direction = segment / max(length, 1.0e-12);
                    distance = max(0.0, length - light.source_radius - 1.0e-5);
                    energy = evaluateUnshadowedPointLight(light, position, direction).irradiance;
                }
                packed = packLocalLightTransmission(connectionTransmission(position, direction, distance, lane));
            }
            local_light_field[localLightConnectionIndex(probe, lightIndex, pc.light_count, lane)] = packed;
            incident[lane] = float4(energy * unpackLocalLightTransmission(packed).xyz, 0.0);
            directions[lane] = float4(direction, 0.0);
        }
        GroupMemoryBarrierWithGroupSync();
        if (pc.audit != 0u && lane < 64u)
        {
            for (uint tap = 0u; tap < taps; ++tap)
                response += projectLocalLightResponse(incident[tap].xyz, directions[tap].xyz, normal);
        }
        GroupMemoryBarrierWithGroupSync();
    }
    // Compare the packed representation to the authoritative original evaluator
    // at the probe. Receiver interpolation is validated separately.
    if (pc.audit != 0u && valid && lane < 64u)
    {
#if RE_FLORA_GLASS_TRANSPORT
        float3 reference = evaluateVoxelGlassVisibleLocalLightIrradiance(
            local_light_info, local_lights, contree_node_data, contree_leaf_data,
            scene_tex, chunk_atlas, position, normal, 0.0, lane);
#else
        float3 reference = evaluateVoxelVisibleLocalLightIrradiance(
            local_light_info, local_lights, contree_node_data, contree_leaf_data,
            scene_tex, position, normal, 0.0, lane);
#endif
        float3 error = abs(reference - response);
        float maxError = max(error.x, max(error.y, error.z));
        float scale = max(reference.x, max(reference.y, reference.z));
        bool failed = !all(isfinite(response)) || !all(isfinite(reference)) ||
            maxError > 1.0e-4 + 1.0e-3 * scale;
        InterlockedAdd(local_light_field_audit[0], 1u);
        if (failed) InterlockedAdd(local_light_field_audit[1], 1u);
        InterlockedMax(local_light_field_audit[2], asuint(maxError));
        if (any(response > float3(0.0))) InterlockedAdd(local_light_field_audit[3], 1u);
    }
}
