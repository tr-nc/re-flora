module raster_tree_shading;

import tracer_types;
import terrain_ray_origin;
import tracer_shadowing;

public [[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
public [[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
public [[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(5,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(12,0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(13,0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(14,0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(17,0)]] Sampler2D shadow_map_tex_for_vsm_ping;

// Shared visibility policy, not a constant ambient fill. The fallback supplies a
// point on an actual exposed triangle and its geometric normal; the original
// path retains its historical smooth-normal receiver when the checkbox is off.
public float rasterTreeSunShadow(float3 leafReceiver, float3 cloudReceiver, float3 normal)
{
    DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(
        float4(leafReceiver,1), float4(leafReceiver,1), float4(cloudReceiver,1), int3(0),
        DIRECT_SUN_SHADOW_SOURCE_ALL, DIRECT_TERRAIN_SHADOW_VSM, DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
    receiver = withReceiverPlane(receiver, normal);
    return directSunShadowTransmittancesVsmLeafCloud(receiver, gui_input,
        shadow_camera_info.view_proj_mat, shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex, leaf_shadow_mask_tex, cloud_shadow_tex).combined;
}

public float3 rasterTreeSunIrradiance()
{
    return sun_info.sun_color * sun_info.sun_luminance * step(0.0, sun_info.sun_dir.y);
}
