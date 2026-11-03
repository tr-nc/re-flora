module local_lighting;

import contree_node;
import marching_result;
import scene_query;
import scene_marching;
import terrain_ray_origin;
import local_emitter;
import local_light_math;
import voxel_types;

public static const uint LOCAL_LIGHT_GPU_ABI_VERSION = 3u;
public static const uint LOCAL_LIGHT_KIND_POINT = 1u;
public static const uint LOCAL_LIGHT_FLAG_DDGI_TRACE_DIAGNOSTICS = 1u << 0u;
public static const uint LOCAL_LIGHT_FLAG_LEGACY_SAND_GLASS = 1u << 1u;
public static const uint LOCAL_LIGHT_FLAG_DISABLED = 1u << 2u;

// Versioned record shared by live direct consumers and immutable DDGI transport snapshots.
// Position, range, and source_radius are authoritative world units in the same space as terrain
// and flora hit positions. source_radius both clamps near-field attenuation and defines the
// spherical endpoint volume owned by an analytic point for exact visibility. Voxel emitters
// instead carry their finite box half-extents in shape_params.xyz (w=1); visibility terminates
// at each emitting surface sample, so the radius never exempts lantern frames. Point intensity is
// scene-linear radiant intensity; the evaluator produces irradiance.
public struct U_LightGpu
{
    public uint abi_version;
    public uint kind;
    public uint stable_id_slot;
    public uint stable_id_generation;
    public float3 position;
    public float range;
    public float3 color;
    public float intensity;
    public float3 direction;
    public float source_radius;
    public float4 shape_params;
};

public struct U_LocalLightInfo
{
    public uint abi_version;
    public uint count;
    public uint capacity;
    public uint overflow_count;
    public uint source_revision_low;
    public uint source_revision_high;
    public uint registry_revision_low;
    public uint registry_revision_high;
    public uint live_revision_low;
    public uint live_revision_high;
    public uint transport_revision;
    public uint flags;
};

public struct B_LocalLights
{
    public U_LightGpu data[];
};

public struct LocalPointLightSample
{
    public float3 irradiance;
    public float3 direction_to_light;
    public float distance_to_light;
};

public struct LocalLightVoxelEvaluation
{
    public float3 irradiance;
    public uint candidates;
    public uint visible;
    public uint occluded;
    public uint irradiance_luma_q8;
};

public uint lightingLuminanceQ8(float3 radiance)
{
    float luminance = dot(radiance, float3(0.2126, 0.7152, 0.0722));
    return uint(clamp(floor(luminance * 256.0 + 0.5), 0.0, 65535.0));
}

public bool localLightingEnabled(U_LocalLightInfo info)
{
    return (info.flags & LOCAL_LIGHT_FLAG_DISABLED) == 0u;
}

public uint localLightCount(U_LocalLightInfo info)
{
    if (!localLightingEnabled(info) || info.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION) return 0u;
    return min(info.count, info.capacity);
}

public bool findLocalLightIndexByStableId(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    uint stableIdSlot,
    uint stableIdGeneration,
    out uint lightIndex)
{
    uint count = localLightCount(info);
    [loop]
    for (uint index = 0u; index < count; ++index)
    {
        U_LightGpu light = lights[index];
        if (light.stable_id_slot == stableIdSlot &&
            light.stable_id_generation == stableIdGeneration)
        {
            lightIndex = index;
            return true;
        }
    }
    lightIndex = count;
    return false;
}

public LocalPointLightSample evaluateUnshadowedPointLight(
    U_LightGpu light, float3 receiverPosition, float3 receiverNormal)
{
    LocalPointLightSample result;
    result.irradiance = float3(0.0);
    result.direction_to_light = float3(0.0);
    result.distance_to_light = 0.0;
    if (light.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION ||
        light.kind != LOCAL_LIGHT_KIND_POINT || light.intensity <= 0.0 ||
        light.range <= 0.0 || light.source_radius <= 0.0)
    {
        return result;
    }

    float3 toLight = light.position - receiverPosition;
    float distanceSquared = dot(toLight, toLight);
    if (distanceSquared <= 1.0e-12) return result;
    float distance = sqrt(distanceSquared);
    if (distance >= light.range) return result;
    float3 direction = toLight / distance;
    float normalLengthSquared = dot(receiverNormal, receiverNormal);
    if (normalLengthSquared <= 1.0e-12) return result;
    float cosine = max(0.0, dot(receiverNormal * rsqrt(normalLengthSquared), direction));
    if (cosine <= 0.0) return result;

    float attenuation = pointLightIncidentWeight(distanceSquared, light.source_radius, light.range);
    result.irradiance = light.color * light.intensity * cosine * attenuation;
    result.direction_to_light = direction;
    result.distance_to_light = distance;
    return result;
}

public bool finiteEmitterOwnsHit(U_LightGpu light, uint material, float3 voxelCenter)
{
    return light.shape_params.w>0.5 && material==VOXEL_TYPE_EMISSIVE &&
        localEmitterContainsVoxel(light.position,light.shape_params.xyz,voxelCenter);
}

public float3 traceVoxelEmitterConnection(
    U_LightGpu light, StructuredBuffer<ContreeNode> nodes, StructuredBuffer<uint> leaves,
    readonly RWTexture3D<uint2> scene, float3 origin, float3 endpoint, uint lane)
{
    float3 segment=endpoint-origin;
    float distance=length(segment);
    float3 direction=segment/max(distance,1.0e-12);
    MarchingResult hit=marchScene(nodes,leaves,scene,origin,direction,1.0/direction,lane);
    return !hit.is_hit || hit.distance>=distance-1.0e-5 ||
        finiteEmitterOwnsHit(light,hit.voxel_type,hit.center_position) ? float3(1.0) : float3(0.0);
}

public float3 traceVoxelGlassEmitterConnection(
    U_LocalLightInfo info, U_LightGpu light, StructuredBuffer<ContreeNode> nodes, StructuredBuffer<uint> leaves,
    readonly RWTexture3D<uint2> scene, readonly RWTexture3D<uint> atlas,
    float3 origin, float3 endpoint, uint lane)
{
    float3 segment=endpoint-origin;
    float distance=length(segment);
    SceneStraightTransport t=traceVoxelStraightTransport(nodes,leaves,scene,atlas,
        origin,segment/max(distance,1.0e-12),max(distance-1.0e-5,1.0e-6),
        (info.flags & LOCAL_LIGHT_FLAG_LEGACY_SAND_GLASS)!=0u,lane);
    bool ownSource=t.terminal.kind==SCENE_QUERY_EVENT_OPAQUE &&
        finiteEmitterOwnsHit(light,t.terminal.to_voxel_type,(float3(t.terminal.to_cell)+0.5)/256.0);
    return t.exhausted!=0u || (t.terminal.kind==SCENE_QUERY_EVENT_OPAQUE && !ownSource)
        ? float3(0.0) : t.transmittance;
}

// One authoritative analytic-point segment policy for exact and sampled consumers.
public float3 traceVoxelPointLightConnection(U_LightGpu light,
    StructuredBuffer<ContreeNode> nodes, StructuredBuffer<uint> leaves,
    readonly RWTexture3D<uint2> scene, float3 origin, float offset, uint lane)
{
    float3 segment=light.position-origin;
    float distance=length(segment);
    if(distance<=light.source_radius) return float3(1.0);
    float3 direction=segment/distance;
    MarchingResult hit=marchScene(nodes,leaves,scene,origin,direction,1.0/direction,lane);
    return !hit.is_hit || hit.distance+max(offset,1.0e-5)>=max(0.0,distance-light.source_radius)
        ? float3(1.0) : float3(0.0);
}

public float3 traceVoxelGlassPointLightConnection(U_LocalLightInfo info,U_LightGpu light,
    StructuredBuffer<ContreeNode> nodes, StructuredBuffer<uint> leaves,
    readonly RWTexture3D<uint2> scene, readonly RWTexture3D<uint> atlas,
    float3 origin, uint lane)
{
    float3 segment=light.position-origin;
    float distance=length(segment);
    if(distance<=light.source_radius) return float3(1.0);
    SceneStraightTransport t=traceVoxelStraightTransport(nodes,leaves,scene,atlas,
        origin,segment/distance,max(0.0,distance-light.source_radius),
        (info.flags & LOCAL_LIGHT_FLAG_LEGACY_SAND_GLASS)!=0u,lane);
    return t.exhausted!=0u || t.terminal.kind==SCENE_QUERY_EVENT_OPAQUE
        ? float3(0.0) : t.transmittance;
}

// Finite-emitter visibility uses the same published scene and finite segments as
// analytic points. No screen-space blur, ambient fill, or shadow-strength floor.
LocalLightVoxelEvaluation evaluateVoxelFiniteEmitter(
    U_LightGpu light,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    float3 receiverPosition, float3 receiverNormal,
    float rayOriginOffsetWorld, uint localInvocationIndex)
{
    LocalLightVoxelEvaluation result = {};
    float3 origin = terrainRayOriginFromPosition(receiverPosition, receiverNormal, rayOriginOffsetWorld);
    [loop]
    for (uint tap = 0u; tap < LOCAL_EMITTER_SAMPLE_COUNT; ++tap)
    {
        LocalEmitterSample sample = sampleLocalEmitter(light.position, light.shape_params.xyz,
            light.range, receiverPosition, receiverNormal, tap);
        if (sample.irradiance_per_intensity <= 0.0) continue;
        result.candidates = 1u;
        result.irradiance += light.color * light.intensity * sample.irradiance_per_intensity *
            traceVoxelEmitterConnection(light, nodeData, leafData, sceneTexture,
                origin, sample.position, localInvocationIndex);
    }
    result.visible = any(result.irradiance > float3(0.0)) ? 1u : 0u;
    result.occluded = result.candidates - result.visible;
    result.irradiance_luma_q8 = lightingLuminanceQ8(result.irradiance);
    return result;
}

public LocalLightVoxelEvaluation evaluateVoxelVisibleLocalLightAtIndex(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    float3 receiverPosition,
    float3 receiverNormal,
    float rayOriginOffsetWorld,
    uint localInvocationIndex,
    uint lightIndex)
{
    LocalLightVoxelEvaluation result;
    result.irradiance = float3(0.0);
    result.candidates = 0u;
    result.visible = 0u;
    result.occluded = 0u;
    result.irradiance_luma_q8 = 0u;
    if (lightIndex < localLightCount(info))
    {
        U_LightGpu light = lights[lightIndex];
        if (light.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION &&
            light.kind == LOCAL_LIGHT_KIND_POINT && light.shape_params.w > 0.5)
            return evaluateVoxelFiniteEmitter(light, nodeData, leafData, sceneTexture,
                receiverPosition, receiverNormal, rayOriginOffsetWorld, localInvocationIndex);
        LocalPointLightSample sample = evaluateUnshadowedPointLight(
            light, receiverPosition, receiverNormal);
        if (!any(sample.irradiance > float3(0.0))) return result;
        result.candidates += 1u;

        float3 shadowOrigin = terrainRayOriginFromPosition(
            receiverPosition, receiverNormal, rayOriginOffsetWorld);
        float3 transmittance = traceVoxelPointLightConnection(light,nodeData,leafData,
            sceneTexture,shadowOrigin,rayOriginOffsetWorld,localInvocationIndex);
        if (any(transmittance > float3(0.0)))
        {
            result.visible += 1u;
            result.irradiance += sample.irradiance;
        }
        else
        {
            result.occluded += 1u;
        }
    }
    // One invocation contributes at most 65535 so a full 32768-ray DDGI batch cannot overflow
    // the u32 atomic accumulator. CPU aggregation widens complete sweeps to u64.
    result.irradiance_luma_q8 = lightingLuminanceQ8(result.irradiance);
    return result;
}

// Exact finite segment visibility for voxel-backed receivers. All production local-light
// consumers call this function so attenuation, units, range clipping, and occlusion stay aligned.
public LocalLightVoxelEvaluation evaluateVoxelVisibleLocalLight(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    float3 receiverPosition,
    float3 receiverNormal,
    float rayOriginOffsetWorld,
    uint localInvocationIndex)
{
    LocalLightVoxelEvaluation result;
    result.irradiance = float3(0.0);
    result.candidates = 0u;
    result.visible = 0u;
    result.occluded = 0u;
    result.irradiance_luma_q8 = 0u;
    uint count = localLightCount(info);
    [loop]
    for (uint lightIndex = 0u; lightIndex < count; ++lightIndex)
    {
        LocalLightVoxelEvaluation lightResult = evaluateVoxelVisibleLocalLightAtIndex(
            info, lights, nodeData, leafData, sceneTexture, receiverPosition,
            receiverNormal, rayOriginOffsetWorld, localInvocationIndex, lightIndex);
        result.irradiance += lightResult.irradiance;
        result.candidates += lightResult.candidates;
        result.visible += lightResult.visible;
        result.occluded += lightResult.occluded;
    }
    result.irradiance_luma_q8 = lightingLuminanceQ8(result.irradiance);
    return result;
}

public float3 evaluateVoxelVisibleLocalLightIrradiance(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    float3 receiverPosition,
    float3 receiverNormal,
    float rayOriginOffsetWorld,
    uint localInvocationIndex)
{
    return evaluateVoxelVisibleLocalLight(
        info, lights, nodeData, leafData, sceneTexture, receiverPosition,
        receiverNormal, rayOriginOffsetWorld, localInvocationIndex).irradiance;
}

// Glass transport is a separate semantic entry point so feature-off shaders keep the legacy
// visibility program and descriptor set exactly. Callers select this only in Glass-specialized
// shader modules.
LocalLightVoxelEvaluation evaluateVoxelGlassFiniteEmitter(
    U_LocalLightInfo info, U_LightGpu light,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    readonly RWTexture3D<uint> chunkAtlas,
    float3 receiverPosition, float3 receiverNormal,
    float rayOriginOffsetWorld, uint localInvocationIndex)
{
    LocalLightVoxelEvaluation result = {};
    float3 origin = terrainRayOriginFromPosition(receiverPosition, receiverNormal, rayOriginOffsetWorld);
    [loop]
    for (uint tap = 0u; tap < LOCAL_EMITTER_SAMPLE_COUNT; ++tap)
    {
        LocalEmitterSample sample = sampleLocalEmitter(light.position, light.shape_params.xyz,
            light.range, receiverPosition, receiverNormal, tap);
        if (sample.irradiance_per_intensity <= 0.0) continue;
        result.candidates = 1u;
        float3 transmittance = traceVoxelGlassEmitterConnection(info, light,
            nodeData, leafData, sceneTexture, chunkAtlas, origin, sample.position, localInvocationIndex);
        result.irradiance += light.color * light.intensity * sample.irradiance_per_intensity * transmittance;
    }
    result.visible = any(result.irradiance > float3(0.0)) ? 1u : 0u;
    result.occluded = result.candidates - result.visible;
    result.irradiance_luma_q8 = lightingLuminanceQ8(result.irradiance);
    return result;
}

public LocalLightVoxelEvaluation evaluateVoxelGlassVisibleLocalLightAtIndex(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    readonly RWTexture3D<uint> chunkAtlas,
    float3 receiverPosition,
    float3 receiverNormal,
    float rayOriginOffsetWorld,
    uint localInvocationIndex,
    uint lightIndex)
{
    LocalLightVoxelEvaluation result;
    result.irradiance = float3(0.0);
    result.candidates = 0u;
    result.visible = 0u;
    result.occluded = 0u;
    result.irradiance_luma_q8 = 0u;
    if (lightIndex < localLightCount(info))
    {
        U_LightGpu light = lights[lightIndex];
        if (light.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION &&
            light.kind == LOCAL_LIGHT_KIND_POINT && light.shape_params.w > 0.5)
            return evaluateVoxelGlassFiniteEmitter(info, light, nodeData, leafData, sceneTexture, chunkAtlas,
                receiverPosition, receiverNormal, rayOriginOffsetWorld, localInvocationIndex);
        LocalPointLightSample sample = evaluateUnshadowedPointLight(
            light, receiverPosition, receiverNormal);
        if (!any(sample.irradiance > float3(0.0))) return result;
        result.candidates += 1u;

        float3 shadowOrigin = terrainRayOriginFromPosition(
            receiverPosition, receiverNormal, rayOriginOffsetWorld);
        float3 transmittance = traceVoxelGlassPointLightConnection(info,light,
            nodeData,leafData,sceneTexture,chunkAtlas,shadowOrigin,localInvocationIndex);
        if (any(transmittance > float3(0.0)))
        {
            result.visible += 1u;
            result.irradiance += sample.irradiance * transmittance;
        }
        else
        {
            result.occluded += 1u;
        }
    }
    result.irradiance_luma_q8 = lightingLuminanceQ8(result.irradiance);
    return result;
}

public LocalLightVoxelEvaluation evaluateVoxelGlassVisibleLocalLight(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    readonly RWTexture3D<uint> chunkAtlas,
    float3 receiverPosition,
    float3 receiverNormal,
    float rayOriginOffsetWorld,
    uint localInvocationIndex)
{
    LocalLightVoxelEvaluation result;
    result.irradiance = float3(0.0);
    result.candidates = 0u;
    result.visible = 0u;
    result.occluded = 0u;
    result.irradiance_luma_q8 = 0u;
    uint count = localLightCount(info);
    [loop]
    for (uint lightIndex = 0u; lightIndex < count; ++lightIndex)
    {
        LocalLightVoxelEvaluation lightResult =
            evaluateVoxelGlassVisibleLocalLightAtIndex(
                info, lights, nodeData, leafData, sceneTexture, chunkAtlas,
                receiverPosition, receiverNormal, rayOriginOffsetWorld,
                localInvocationIndex, lightIndex);
        result.irradiance += lightResult.irradiance;
        result.candidates += lightResult.candidates;
        result.visible += lightResult.visible;
        result.occluded += lightResult.occluded;
    }
    result.irradiance_luma_q8 = lightingLuminanceQ8(result.irradiance);
    return result;
}

public float3 evaluateVoxelGlassVisibleLocalLightIrradiance(
    U_LocalLightInfo info,
    StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    readonly RWTexture3D<uint> chunkAtlas,
    float3 receiverPosition,
    float3 receiverNormal,
    float rayOriginOffsetWorld,
    uint localInvocationIndex)
{
    return evaluateVoxelGlassVisibleLocalLight(
        info, lights, nodeData, leafData, sceneTexture, chunkAtlas,
        receiverPosition, receiverNormal, rayOriginOffsetWorld,
        localInvocationIndex).irradiance;
}
