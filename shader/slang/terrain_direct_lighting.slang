module terrain_direct_lighting;

import contree_node;
import local_lighting;
import terrain_ray_origin;
import tracer_shadowing;
import tracer_types;

// Primary terrain and secondary Glass hits share the same world-space direct
// light response. Screen visibility must never select another lighting model.
public float3 terrainDirectIrradiance(
    U_GuiInput gui, U_SunInfo sun, U_ShadowCameraInfo shadowCamera,
    U_LocalLightInfo lightInfo, StructuredBuffer<U_LightGpu> lights,
    StructuredBuffer<ContreeNode> nodes, StructuredBuffer<uint> leaves,
    readonly RWTexture3D<uint2> scene,
    readonly RWTexture3D<uint> atlas,
    float3 voxelCenter, float3 normal, bool glassTransport,
    uint invocation, out DirectSunShadowTransmittances shadow)
{
    shadow.terrain = 1.0;
    shadow.leaf = 1.0;
    shadow.combined = 1.0;
    float3 sunIrradiance = float3(0.0);
    float cosine = max(0.0, dot(normalize(sun.sun_dir), normal));
    if (cosine > 0.0)
    {
        shadow = sampleDirectSunShadow(voxelSunShadowReceiver(
            voxelCenter, normal, gui.terrain_ray_origin_offset_world),
            gui, shadowCamera);
        sunIrradiance = shadow.combined * cosine * sun.sun_color *
            sun.sun_luminance * step(0.0, sun.sun_dir.y);
    }
    float3 receiver = terrainVoxelSurfacePositionAlongNormal(voxelCenter, normal);
    float3 localIrradiance;
    if (glassTransport)
        localIrradiance = evaluateVoxelGlassVisibleLocalLightIrradiance(
            lightInfo, lights, nodes, leaves, scene, atlas, receiver, normal,
            gui.terrain_ray_origin_offset_world, invocation);
    else
        localIrradiance = evaluateVoxelVisibleLocalLightIrradiance(
            lightInfo, lights, nodes, leaves, scene, receiver, normal,
            gui.terrain_ray_origin_offset_world, invocation);
    return sunIrradiance + localIrradiance;
}
