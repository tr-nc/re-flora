import ray;
import tracer_types;

static const uint LENS_FLARE_GROUP_SIZE = 8u;
static const uint LENS_FLARE_DOWNSAMPLE_FACTOR = 4u;
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
    uint depthWidth;
    uint depthHeight;
    gfx_depth_tex.GetDimensions(depthWidth, depthHeight);
    uint sampleWidth = max(depthWidth / LENS_FLARE_DOWNSAMPLE_FACTOR, 1u);
    uint sampleHeight = max(depthHeight / LENS_FLARE_DOWNSAMPLE_FACTOR, 1u);
    int2 sampleCoordinate = int2(dispatchThreadId.xy);
    if (any(sampleCoordinate >= int2(sampleWidth, sampleHeight)))
    {
        return;
    }

    float2 screenUv = (float2(sampleCoordinate) + float2(0.5)) /
                      float2(sampleWidth, sampleHeight);
    float alpha = sampleSunAlpha(screenUv);
    if (alpha <= 0.001)
    {
        return;
    }

    int2 depthCoordinate = int2(clamp(
        screenUv * float2(depthWidth, depthHeight), float2(0.0),
        float2(int2(depthWidth, depthHeight) - int2(1))));
    float graphicsDepth = gfx_depth_tex.Load(int3(depthCoordinate, 0)).r;
    float computeDepth = compute_depth_tex[depthCoordinate];
    if (graphicsDepth >= 1.0 && computeDepth >= 1.0)
    {
        uint previousCount;
        InterlockedAdd(lens_flare_visible_count_tex[0], 1u, previousCount);
    }
}
