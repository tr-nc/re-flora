module local_lighting;

import contree_node;
import marching_result;
import scene_marching;
import terrain_ray_origin;

public static const uint LOCAL_LIGHT_GPU_ABI_VERSION = 2u;
public static const uint LOCAL_LIGHT_KIND_POINT = 1u;
public static const uint LOCAL_LIGHT_FLAG_DDGI_TRACE_DIAGNOSTICS = 1u << 0u;

// Versioned record shared by live direct consumers and immutable DDGI transport snapshots.
// Position, range, and source_radius are authoritative world units in the same space as terrain
// and flora hit positions. source_radius both clamps near-field attenuation and defines the
// spherical endpoint volume owned by the emitter for exact visibility. Point intensity is
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

public uint localLightCount(U_LocalLightInfo info)
{
    if (info.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION) return 0u;
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

    float normalizedDistance = distance / light.range;
    float rangeBase = max(0.0, 1.0 - pow(normalizedDistance, 4.0));
    float rangeWindow = rangeBase * rangeBase;
    float attenuation = rangeWindow /
        max(distanceSquared, light.source_radius * light.source_radius);
    result.irradiance = light.color * light.intensity * cosine * attenuation;
    result.direction_to_light = direction;
    result.distance_to_light = distance;
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
        LocalPointLightSample sample = evaluateUnshadowedPointLight(
            light, receiverPosition, receiverNormal);
        if (!any(sample.irradiance > float3(0.0))) return result;
        result.candidates += 1u;

        float3 shadowOrigin = terrainRayOriginFromSurface(
            receiverPosition, receiverNormal, rayOriginOffsetWorld);
        float3 shadowToLight = light.position - shadowOrigin;
        float segmentLength = length(shadowToLight);
        if (segmentLength <= light.source_radius)
        {
            result.visible += 1u;
            result.irradiance += sample.irradiance;
            return result;
        }
        float3 shadowDirection = shadowToLight / segmentLength;
        MarchingResult shadowHit = marchScene(
            nodeData, leafData, sceneTexture, shadowOrigin,
            shadowDirection, 1.0 / shadowDirection,
            localInvocationIndex);
        // A hit in the final source-radius sphere belongs to the emitter geometry itself. A hit
        // before that endpoint volume remains an independent blocker.
        float endpointStart = max(0.0, segmentLength - light.source_radius);
        float visibility = !shadowHit.is_hit ||
            shadowHit.distance + max(rayOriginOffsetWorld, 1.0e-5) >= endpointStart
            ? 1.0 : 0.0;
        if (visibility > 0.0)
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
