import ray;
import tracer_types;

static const uint LENS_FLARE_GROUP_SIZE = 8u;
static const float MINIMUM_VIEW_DEPTH = 1.0e-5;
static const float MINIMUM_SUN_SIZE = 1.0e-4;

[[vk::binding(0, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(1, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(2, 0)]] Sampler2D gfx_depth_tex;

[[vk::binding(3, 0)]]
[[vk::image_format("r32f")]]
readonly RWTexture2D<float> compute_depth_tex;

[[vk::binding(4, 0)]] Sampler2D sun_sprite_tex;

[[vk::binding(5, 0)]]
[[vk::image_format("r32ui")]]
RWTexture1D<uint> lens_flare_visible_count_tex;

float sampleSunAlpha(float2 screenUv)
{
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    float3 viewDirection = normalize(ray.direction);
    if (dot(viewDirection, sun_info.sun_dir) <= 0.0)
    {
        return 0.0;
    }

    float3 viewDirectionView = normalize(
        mul(camera_info.view_mat, float4(viewDirection, 0.0)).xyz);
    float3 sunDirectionView = normalize(
        mul(camera_info.view_mat, float4(sun_info.sun_dir, 0.0)).xyz);
    float viewDepth = -viewDirectionView.z;
    float sunDepth = -sunDirectionView.z;
    if (viewDepth <= MINIMUM_VIEW_DEPTH ||
        sunDepth <= MINIMUM_VIEW_DEPTH)
    {
        return 0.0;
    }

    float2 viewProjection = viewDirectionView.xy / viewDepth;
    float2 sunProjection = sunDirectionView.xy / sunDepth;
    float2 uv = (viewProjection - sunProjection) /
                max(sun_info.sun_size, MINIMUM_SUN_SIZE) + float2(0.5);
    if (any(uv < float2(0.0)) || any(uv > float2(1.0)))
    {
        return 0.0;
    }

    return sun_sprite_tex.SampleLevel(uv, 0.0).a;
}

[shader("compute")]
[numthreads(LENS_FLARE_GROUP_SIZE, LENS_FLARE_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    gfx_depth_tex.GetDimensions(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= int2(width, height)))
    {
        return;
    }

    float2 screenUv = (float2(coordinate) + float2(0.5)) /
                      float2(width, height);
    float alpha = sampleSunAlpha(screenUv);
    if (alpha <= 0.001)
    {
        return;
    }

    float graphicsDepth = gfx_depth_tex.Load(int3(coordinate, 0)).r;
    float computeDepth = compute_depth_tex[coordinate];
    if (graphicsDepth >= 1.0 && computeDepth >= 1.0)
    {
        uint previousCount;
        InterlockedAdd(lens_flare_visible_count_tex[0], 1u, previousCount);
    }
}
