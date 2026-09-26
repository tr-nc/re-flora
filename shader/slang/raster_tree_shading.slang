module raster_tree_shading;

import tracer_types;
import terrain_ray_origin;
import tracer_shadowing;

public [[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
public [[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
public [[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(5,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;

// Shared visibility policy, not a constant ambient fill. The fallback supplies a
// point on an actual exposed triangle and its geometric normal; the original
// path retains its historical smooth-normal receiver when the checkbox is off.
public float rasterTreeSunShadow(float3 voxelCenter, float3 surfacePosition, float3 normal)
{
    return sampleDirectSunShadow(
        voxelSurfaceSunShadowReceiver(voxelCenter, surfacePosition, normal,
            gui_input.terrain_ray_origin_offset_world),
        gui_input, shadow_camera_info).combined;
}

// Exposed triangle samples already are surface points, not voxel centers.
public float rasterTreeSurfaceSunShadow(float3 position, float3 normal)
{
    return sampleDirectSunShadow(
        surfaceSunShadowReceiver(position, normal, gui_input.terrain_ray_origin_offset_world),
        gui_input, shadow_camera_info).combined;
}

public float3 rasterTreeSunIrradiance()
{
    return sun_info.sun_color * sun_info.sun_luminance * step(0.0, sun_info.sun_dir.y);
}
