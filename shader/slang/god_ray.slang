import composition_types;
import noise_tex;
import projection;
import ray;
import tracer_types;

static const uint GOD_RAY_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(1, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_EnvInfo> env_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_GodRayInfo> god_ray_info;
[[vk::binding(4, 0)]] Sampler2D gfx_depth_tex;

[[vk::binding(5, 0)]]
[[vk::image_format("r32f")]]
readonly RWTexture2D<float> compute_depth_tex;

[[vk::binding(6, 0)]] Sampler2D shadow_map_tex;

[[vk::binding(7, 0)]]
[[vk::image_format("r32f")]]
WTexture2D<float> god_ray_output_tex;

[[vk::binding(0, 1)]] Sampler2DArray scalar_bn;
[[vk::binding(1, 1)]] Sampler2DArray unit_vec2_bn;
[[vk::binding(2, 1)]] Sampler2DArray unit_vec3_bn;
[[vk::binding(3, 1)]] Sampler2DArray weighted_cosine_bn;
[[vk::binding(4, 1)]] Sampler2DArray fast_unit_vec3_bn;
[[vk::binding(5, 1)]] Sampler2DArray fast_weighted_cosine_bn;

float getDepth(int2 coordinate)
{
    float graphicsDepth = gfx_depth_tex.Load(int3(coordinate, 0)).r;
    float computeDepth = compute_depth_tex[coordinate];
    return min(graphicsDepth, computeDepth);
}

[shader("compute")]
[numthreads(GOD_RAY_GROUP_SIZE, GOD_RAY_GROUP_SIZE, 1)]
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
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    float depth = getDepth(coordinate);

    float3 nearPoint = projectScreenUvToWorld(
        screenUv, 0.0, camera_info.view_proj_mat_inv);
    float3 hitFarPoint = projectScreenUvToWorld(
        screenUv, depth, camera_info.view_proj_mat_inv);
    float realDepth = length(hitFarPoint - nearPoint);
    float usedDepth = min(realDepth, god_ray_info.max_depth);
    float stepSize = usedDepth / float(god_ray_info.max_checks);

    int3 seed = getBlueNoiseSeed(
        dispatchThreadId.xy, env_info.frame_serial_idx);
    float randomValue = randomFloat(scalar_bn, seed);
    float shadowVisibility = 0.0;

    for (uint i = 0u; i < god_ray_info.max_checks; ++i)
    {
        float currentDepth = (randomValue + float(i)) * stepSize;
        if (currentDepth >= usedDepth)
        {
            break;
        }

        float4 lookupPosition = float4(
            ray.origin + ray.direction * currentDepth, 1.0);
        float4 lightSpace = mul(
            shadow_camera_info.view_proj_mat, lookupPosition);
        float3 ndc = lightSpace.xyz / lightSpace.w;
        float2 uv = ndc.xy * 0.5 + 0.5;
        float shadowDepth = shadow_map_tex.SampleLevel(uv, 0.0).r;
        if (shadowDepth > ndc.z)
        {
            float mistWeight = exp(-lookupPosition.y * 2.0);
            shadowVisibility += mistWeight * stepSize;
        }
    }

    god_ray_output_tex.Store(coordinate, shadowVisibility);
}
