import hash;
import sunlight;
import tracer_types;

static const uint LENS_FLARE_GROUP_SIZE = 8u;
static const float PI = 3.14159265;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_CameraInfo> camera_info;

[[vk::binding(4, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture1D<uint> lens_flare_visible_count_tex;

[[vk::binding(5, 0)]]
[[vk::image_format("r11f_g11f_b10f")]]
WTexture2D<float4> lens_flare_full_output_tex;

float lensFlareNoise(float value)
{
    return hash11(value);
}

float lensFlareNoise(float2 value)
{
    return hash12(value);
}

float3 lensFlarePattern(float2 uv, float2 position)
{
    float2 main = uv - position;
    float2 distortedUv = uv * length(uv);

    float angle = atan2(main.x, main.y);
    float distance = pow(length(main), 0.1);
    float noise = lensFlareNoise(float2(angle * 16.0, distance * 32.0));

    float f0 = 1.0 / (length(uv - position) * 16.0 + 1.0);
    f0 += f0 *
          (sin(lensFlareNoise(
               sin(angle * 2.0 + position.x) * 4.0 -
               cos(angle * 3.0 + position.y)) * 16.0) * 0.1 +
           distance * 0.1 + 0.8);

    float f1 = max(0.01 - pow(length(uv + 1.2 * position), 1.9), 0.0) * 7.0;

    float f2 = max(1.0 / (1.0 + 32.0 * pow(length(distortedUv + 0.8 * position), 2.0)), 0.0) * 0.25;
    float f22 = max(1.0 / (1.0 + 32.0 * pow(length(distortedUv + 0.85 * position), 2.0)), 0.0) * 0.23;
    float f23 = max(1.0 / (1.0 + 32.0 * pow(length(distortedUv + 0.9 * position), 2.0)), 0.0) * 0.21;

    float2 uvx = lerp(uv, distortedUv, -0.5);
    float f4 = max(0.01 - pow(length(uvx + 0.4 * position), 2.4), 0.0) * 6.0;
    float f42 = max(0.01 - pow(length(uvx + 0.45 * position), 2.4), 0.0) * 5.0;
    float f43 = max(0.01 - pow(length(uvx + 0.5 * position), 2.4), 0.0) * 3.0;

    uvx = lerp(uv, distortedUv, -0.4);
    float f5 = max(0.01 - pow(length(uvx + 0.2 * position), 5.5), 0.0) * 2.0;
    float f52 = max(0.01 - pow(length(uvx + 0.4 * position), 5.5), 0.0) * 2.0;
    float f53 = max(0.01 - pow(length(uvx + 0.6 * position), 5.5), 0.0) * 2.0;

    uvx = lerp(uv, distortedUv, -0.5);
    float f6 = max(0.01 - pow(length(uvx - 0.3 * position), 1.6), 0.0) * 6.0;
    float f62 = max(0.01 - pow(length(uvx - 0.325 * position), 1.6), 0.0) * 3.0;
    float f63 = max(0.01 - pow(length(uvx - 0.35 * position), 1.6), 0.0) * 5.0;

    float3 color = float3(0.0);
    color.r += f2 + f4 + f5 + f6;
    color.g += f22 + f42 + f52 + f62;
    color.b += f23 + f43 + f53 + f63;
    color = color * 1.3 - float3(length(distortedUv) * 0.05);
    color += float3(f0 * 0.22 + f1 * 0.25 + noise * 0.02);
    return color;
}

float3 correctLensFlareColor(float3 color, float factor, float secondFactor)
{
    float weight = color.x + color.y + color.z;
    return lerp(color, float3(weight) * factor, weight * secondFactor);
}

[shader("compute")]
[numthreads(LENS_FLARE_GROUP_SIZE, LENS_FLARE_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    lens_flare_full_output_tex.GetDimensions(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= int2(width, height)))
    {
        return;
    }

    float2 lensFlareUv = (float2(coordinate) + float2(0.5)) /
                         float2(width, height);
    float3 sunWorldPosition = camera_info.pos.xyz +
                              sun_info.sun_dir * 10000.0;
    float4 sunClipPosition = mul(
        camera_info.view_proj_mat, float4(sunWorldPosition, 1.0));
    if (sunClipPosition.w <= 0.0)
    {
        lens_flare_full_output_tex.Store(coordinate, float4(0.0));
        return;
    }

    float2 sunUv = sunClipPosition.xy / sunClipPosition.w * 0.5 + 0.5;
    float2 sunDistanceFromCenter = abs(sunUv - float2(0.5));
    float edgeFactor = 1.0 - smoothstep(
        0.45, 0.8, max(sunDistanceFromCenter.x, sunDistanceFromCenter.y));
    if (edgeFactor <= 0.0)
    {
        lens_flare_full_output_tex.Store(coordinate, float4(0.0));
        return;
    }

    uint visibleCount = lens_flare_visible_count_tex[0];
    if (visibleCount == 0u)
    {
        lens_flare_full_output_tex.Store(coordinate, float4(0.0));
        return;
    }

    float sunRadiusPixels = 0.5 * max(sun_info.sun_size, 0.0001) *
                            camera_info.proj_mat[1][1] * float(height);
    float estimatedTotalPixels = gui_input.lens_flare_sun_pixel_scale * PI *
                                 sunRadiusPixels * sunRadiusPixels;
    float visibleRatio = clamp(
        float(visibleCount) / max(estimatedTotalPixels, 1.0), 0.0, 1.0);

    float2 uv = lensFlareUv - float2(0.5);
    float2 position = sunUv - float2(0.5);
    float aspect = float(width) / float(height);
    uv.x *= aspect;
    position.x *= aspect;

    float3 flare = float3(1.4, 1.2, 1.0) *
                   lensFlarePattern(uv, position);
    flare = correctLensFlareColor(flare, 0.5, 0.1);

    float sunVisibility = sunLuminanceFromDirection(sun_info.sun_dir, 1.0);
    float intensity = gui_input.lens_flare_intensity * sunVisibility *
                      edgeFactor * visibleRatio;
    float3 lensFlareColor = max(flare, float3(0.0)) * intensity;
    lens_flare_full_output_tex.Store(
        coordinate, float4(lensFlareColor, 1.0));
}
