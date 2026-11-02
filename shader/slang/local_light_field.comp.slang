#ifndef RE_FLORA_GLASS_TRANSPORT
#define RE_FLORA_GLASS_TRANSPORT 0
#endif

import contree_node;
import ddgi_types;
import ddgi_octahedral;
import local_lighting;
import local_emitter;
import local_light_math;
import marching_result;
import scene_marching;
#if RE_FLORA_GLASS_TRANSPORT
import scene_query;
#endif

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

// Frame-owned direct response, never bound to recursive DDGI transport.
[[vk::binding(0, 1)]] RWTexture2D<float4> local_light_field;
[[vk::binding(2, 1)]] RWStructuredBuffer<uint> local_light_field_audit;
[[vk::binding(1, 1)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout> ddgi_probe_metadata;

struct LocalLightFieldPushConstants
{
    uint first_probe;
    uint probe_count;
    uint tile_columns;
    uint audit;
};
[[vk::push_constant]] ConstantBuffer<LocalLightFieldPushConstants> pc;

// Iterate lights in shared-memory batches; no additional hard-coded light cap.
// A point light uses one connection; finite emitters retain all quadrature taps.
groupshared float4 incident[LOCAL_EMITTER_SAMPLE_COUNT];
groupshared float4 directions[LOCAL_EMITTER_SAMPLE_COUNT];

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
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probe];
    bool valid = metadata.state_and_reserved.x == DDGI_PROBE_STATE_VALID;
    float3 position = metadata.actual_position_and_clearance.xyz;
    // Project interior and gutters directly using the canonical gutter mapping.
    uint2 texel = uint2(lane % 10u, lane / 10u);
    bool border = any(texel == uint2(0u)) || any(texel == uint2(9u));
    uint2 source = border ? ddgiGutterSource(texel, 8u) : texel;
    float3 normal = octDecode(((float2(source) - 0.5) / 8.0) * 2.0 - 1.0);
    float3 response = float3(0.0);
    uint count = valid ? localLightCount(local_light_info) : 0u;
    for (uint lightIndex = 0u; lightIndex < count; ++lightIndex)
    {
        U_LightGpu light = local_lights[lightIndex];
        bool finiteEmitter = light.shape_params.w > 0.5;
        uint taps = finiteEmitter ? LOCAL_EMITTER_SAMPLE_COUNT : 1u;
        if (lane < LOCAL_EMITTER_SAMPLE_COUNT)
        {
            float3 energy = float3(0.0);
            float3 direction = float3(0.0);
            float distance = 0.0;
            bool supported = light.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION &&
                light.kind == LOCAL_LIGHT_KIND_POINT && light.intensity > 0.0;
            if (supported && lane < taps)
            {
                if (finiteEmitter)
                {
                    LocalEmitterSample sample = sampleLocalEmitterIncident(
                        light.position, light.shape_params.xyz, light.range, position, lane);
                    float3 segment = sample.position - position;
                    float length = sqrt(dot(segment, segment));
                    direction = segment / max(length, 1.0e-12);
                    distance = max(length - 1.0e-5, 1.0e-6);
                    energy = light.color * light.intensity * sample.irradiance_per_intensity;
                }
                else
                {
                    float3 segment = light.position - position;
                    float length = sqrt(dot(segment, segment));
                    direction = segment / max(length, 1.0e-12);
                    LocalPointLightSample sample = evaluateUnshadowedPointLight(light, position, direction);
                    energy = sample.irradiance;
                    distance = max(0.0, length - light.source_radius);
                }
                if (any(energy > float3(0.0)))
                    energy *= connectionTransmission(position, direction, distance, lane);
            }
            incident[lane] = float4(energy, 0.0);
            directions[lane] = float4(direction, 0.0);
        }
        GroupMemoryBarrierWithGroupSync();
        if (lane < 100u)
        {
            for (uint tap = 0u; tap < taps; ++tap)
                response += projectLocalLightResponse(incident[tap].xyz, directions[tap].xyz, normal);
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if (lane < 100u)
    {
        // Same authored response units as the replaced local-light evaluator.
        // This is NOT a uniform-sphere Monte Carlo estimate: no 4/N or extra pi.
        local_light_field[ddgiTileOrigin(probe, pc.tile_columns, 10u) + texel] = float4(response, 1.0);
        // Opt-in GPU evidence through the original evaluator, at the same volume
        // position (no invented surface-origin bias). Never enabled in benchmarks.
        if (pc.audit != 0u && valid && !border)
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
}
