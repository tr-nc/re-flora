import clouds;
import color;
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
[[vk::binding(12, 0)]] [[vk::image_format("r11f_g11f_b10f")]]
RWTexture2D<float4> composited_tex;
[[vk::binding(13, 0)]] [[vk::image_format("r11f_g11f_b10f")]]
readonly RWTexture2D<float4> lens_flare_output_tex;
[[vk::binding(14, 0)]] Sampler2D sun_sprite_tex;
[[vk::binding(15, 0)]] Sampler2D cloud_output_tex;

float3 sampleLensFlare(float2 screenUv)
{
    uint width;
    uint height;
    lens_flare_output_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    float2 texel = screenUv * float2(size) - float2(0.5);
    int2 base = int2(floor(texel));
    float2 fraction = frac(texel);
    int2 maximum = size - int2(1);
    int2 c00 = clamp(base, int2(0), maximum);
    int2 c10 = clamp(base + int2(1, 0), int2(0), maximum);
    int2 c01 = clamp(base + int2(0, 1), int2(0), maximum);
    int2 c11 = clamp(base + int2(1, 1), int2(0), maximum);
    float3 top = lerp(lens_flare_output_tex[c00].rgb,
                      lens_flare_output_tex[c10].rgb, fraction.x);
    float3 bottom = lerp(lens_flare_output_tex[c01].rgb,
                         lens_flare_output_tex[c11].rgb, fraction.x);
    return lerp(top, bottom, fraction.y);
}

float sampleGodRay(float2 screenUv)
{
    uint width;
    uint height;
    god_ray_output_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    float2 texel = screenUv * float2(size) - float2(0.5);
    int2 base = int2(floor(texel));
    float2 fraction = frac(texel);
    int2 maximum = size - int2(1);
    int2 c00 = clamp(base, int2(0), maximum);
    int2 c10 = clamp(base + int2(1, 0), int2(0), maximum);
    int2 c01 = clamp(base + int2(0, 1), int2(0), maximum);
    int2 c11 = clamp(base + int2(1, 1), int2(0), maximum);
    float top = lerp(god_ray_output_tex[c00], god_ray_output_tex[c10],
                     fraction.x);
    float bottom = lerp(god_ray_output_tex[c01], god_ray_output_tex[c11],
                        fraction.x);
    return lerp(top, bottom, fraction.y);
}

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
    finalColor += sampleLensFlare(screenUv);

    float sunVisibility = sunLuminanceFromDirection(sun_info.sun_dir, 1.0);
    float godRayWeight = sampleGodRay(screenUv) * god_ray_info.weight;
    godRayWeight *= sunVisibility;
    float3 godRayColor = srgbToLinear(god_ray_info.color);
    finalColor = lerp(finalColor, godRayColor, godRayWeight);

    composited_tex[coordinate] = float4(finalColor, 1.0);
}
