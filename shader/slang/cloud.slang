import clouds;
import noise_tex;
import ray;
import tracer_types;

static const uint CLOUD_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(5, 0)]] ConstantBuffer<U_EnvInfo> env_info;

[[vk::binding(15, 0)]]
[[vk::image_format("rgba16f")]]
WTexture2D<float4> cloud_raw_tex;

[[vk::binding(0, 1)]] Sampler2DArray scalar_bn;

[shader("compute")]
[numthreads(CLOUD_GROUP_SIZE, CLOUD_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    cloud_raw_tex.GetDimensions(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= int2(width, height)))
    {
        return;
    }

    float2 screenUv = (float2(coordinate) + float2(0.5)) /
                      float2(width, height);
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    float jitter = randomFloat(
        scalar_bn,
        getBlueNoiseSeed(dispatchThreadId.xy, env_info.frame_serial_idx));
    CloudInputs inputs = makeCloudInputs(
        gui_input, sun_info, camera_info, env_info);
    CloudRaymarchResult result = computeVolumetricCloudsWithJitter(
        inputs, ray.direction, 0u, jitter);
    cloud_raw_tex.Store(
        coordinate, float4(result.color, result.alpha));
}
