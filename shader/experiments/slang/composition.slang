import color;
import composition_sky;
import composition_types;
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
[[vk::binding(9, 0)]] [[vk::image_format("r11f_g11f_b10f")]]
readonly RWTexture2D<float4> denoiser_spatial_pong_tex;
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

float3 applyScreenClouds(float3 skyColor, float2 screenUv)
{
    if (gui_input.clouds_enabled == 0u) return skyColor;
    float4 clouds = cloud_output_tex.SampleLevel(screenUv, 0.0);
    return lerp(skyColor, clouds.rgb, clamp(clouds.a, 0.0, 1.0));
}

float3 combineColors(float rasterDepth, float computeDepth, float4 rasterColor,
                     float3 denoisedColor, float2 screenUv)
{
    bool hasRaster = rasterColor.a > 0.0 && rasterDepth < 1.0;
    bool hasRayTracing = computeDepth < 1.0;
    float rayTracingDepth = hasRayTracing ? computeDepth : 1.0;
    float alpha = clamp(rasterColor.a, 0.0, 1.0);

    if (hasRayTracing)
    {
        if (!hasRaster || rasterDepth > rayTracingDepth) return denoisedColor;
        return rasterColor.rgb + denoisedColor * (1.0 - alpha);
    }
    if (hasRaster && alpha >= 0.999) return rasterColor.rgb;

    float3 viewDirection = normalize(generateCameraRay(screenUv,
                                                       camera_info.view_proj_mat_inv).direction);
    float3 skyColor = applyScreenClouds(
        computeSkyWithSunAndStars(viewDirection, sun_info, camera_info,
                                  starlight_info, sun_sprite_tex),
        screenUv);
    if (!hasRaster) return skyColor;
    return rasterColor.rgb + skyColor * (1.0 - alpha);
}

float3 sampleLensFlare(float2 screenUv)
{
    uint width;
    uint height;
    lens_flare_output_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    int2 coordinate = int2(clamp(screenUv * float2(size), float2(0.0),
                                 float2(size - int2(1))));
    return lens_flare_output_tex[coordinate].rgb;
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
    float3 denoisedColor = denoiser_spatial_pong_tex[coordinate].rgb;
    float3 finalColor = combineColors(rasterDepth, computeDepth, rasterColor,
                                      denoisedColor, screenUv);
    finalColor += sampleLensFlare(screenUv);

    float sunVisibility = sunLuminanceFromDirection(sun_info.sun_dir, 1.0);
    float godRayWeight = god_ray_output_tex[coordinate] * god_ray_info.weight;
    godRayWeight *= sunVisibility;
    float3 godRayColor = srgbToLinear(god_ray_info.color);
    finalColor = lerp(finalColor, godRayColor, godRayWeight);

    composited_tex[coordinate] = float4(finalColor, 1.0);
}
