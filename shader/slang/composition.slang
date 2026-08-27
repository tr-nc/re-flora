import clouds;
import color;
import composition_camera_effects;
import composition_cloud_reflection;
import composition_scene;
import composition_terrarium_glass;
import composition_terrarium_panels;
import composition_types;
import packer;
import ray;
import sunlight;
import tracer_types;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_GodRayInfo> god_ray_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(5, 0)]] ConstantBuffer<U_EnvInfo> env_info;
[[vk::binding(6, 0)]] ConstantBuffer<U_StarlightInfo> starlight_info;
[[vk::binding(7, 0)]] Sampler2D gfx_output_tex;
[[vk::binding(8, 0)]] Sampler2D gfx_depth_tex;
[[vk::binding(9, 0)]] [[vk::image_format("r32ui")]]
readonly RWTexture2D<uint> compute_output_tex;
[[vk::binding(10, 0)]] [[vk::image_format("r32f")]]
readonly RWTexture2D<float> compute_depth_tex;
[[vk::binding(11, 0)]] [[vk::image_format("r32f")]]
readonly RWTexture2D<float> god_ray_output_tex;
[[vk::binding(12, 0)]] [[vk::image_format("rgba16f")]]
RWTexture2D<float4> composited_tex;
[[vk::binding(13, 0)]] [[vk::image_format("rgba16f")]]
readonly RWTexture2D<float4> lens_flare_output_tex;
[[vk::binding(14, 0)]] Sampler2D sun_sprite_tex;
[[vk::binding(15, 0)]] Sampler2D cloud_output_tex;
[[vk::binding(16, 0)]] [[vk::image_format("rgba16f")]]
RWTexture2D<float4> unified_opaque_hdr_tex;
[[vk::binding(17, 0)]] [[vk::image_format("r32f")]]
RWTexture2D<float> unified_opaque_depth_tex;
[[vk::binding(18, 0)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> opaque_provenance_tex;

static const uint OPAQUE_PROVENANCE_VOXEL = 1u;
static const uint OPAQUE_PROVENANCE_RASTER = 2u;
static const uint OPAQUE_PROVENANCE_SKY = 3u;
static const uint OPAQUE_PROVENANCE_UNSUPPORTED_RASTER = 4u;

[numthreads(8, 8, 1)]
[shader("compute")]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint outputWidth;
    uint outputHeight;
    composited_tex.GetDimensions(outputWidth, outputHeight);
    if (any(dispatchThreadId.xy >= uint2(outputWidth, outputHeight))) return;

    int2 coordinate = int2(dispatchThreadId.xy);
    float2 screenUv = (float2(dispatchThreadId.xy) + 0.5) /
                      float2(outputWidth, outputHeight);

    float rasterDepth = gfx_depth_tex.Load(int3(coordinate, 0)).r;
    float computeDepth = compute_depth_tex[coordinate];
    float4 rasterColor = gfx_output_tex.Load(int3(coordinate, 0));
    float3 terrainColor = unpackRgbe(compute_output_tex[coordinate]);
    float3 finalColor = combineSceneColors(
        rasterDepth, computeDepth, rasterColor, terrainColor, screenUv,
        gui_input, sun_info, camera_info, starlight_info,
        sun_sprite_tex, cloud_output_tex);

    if (shading_info.glass_experiment_enabled != 0u)
    {
        bool hasRaster = rasterColor.a > 0.0 && rasterDepth < 1.0;
        bool hasVoxel = computeDepth < 1.0;
        bool rasterIsNearest = hasRaster && (!hasVoxel || rasterDepth <= computeDepth);
        uint provenance = OPAQUE_PROVENANCE_SKY;
        float opaqueDepth = 1.0;
        if (rasterIsNearest)
        {
            opaqueDepth = rasterDepth;
            provenance = rasterColor.a >= 0.999
                ? OPAQUE_PROVENANCE_RASTER
                : OPAQUE_PROVENANCE_UNSUPPORTED_RASTER;
        }
        else if (hasVoxel)
        {
            opaqueDepth = computeDepth;
            provenance = OPAQUE_PROVENANCE_VOXEL;
        }
        unified_opaque_hdr_tex[coordinate] = float4(finalColor, 1.0);
        unified_opaque_depth_tex[coordinate] = opaqueDepth;
        opaque_provenance_tex[coordinate] = provenance;
        return;
    }

    finalColor += sampleLensFlare(screenUv, lens_flare_output_tex);

    float sunVisibility = sunLuminanceFromDirection(sun_info.sun_dir, 1.0);
    float godRayWeight = sampleGodRay(screenUv, god_ray_output_tex) * god_ray_info.weight;
    godRayWeight *= sunVisibility;
    float3 godRayColor = srgbToLinear(god_ray_info.color);
    finalColor = lerp(finalColor, godRayColor, godRayWeight);

    composited_tex[coordinate] = float4(finalColor, 1.0);
}
