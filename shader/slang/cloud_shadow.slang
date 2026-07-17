import clouds;
import noise_tex;
import ray;
import tracer_types;

static const uint CLOUD_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5, 0)]] ConstantBuffer<U_EnvInfo> env_info;

[[vk::binding(6, 0)]]
[[vk::image_format("r16f")]]
WTexture2D<float> cloud_shadow_raw_tex;

[[vk::binding(0, 1)]] Sampler2DArray scalar_bn;

bool cloudShadowLayerInterval(CloudInputs inputs, Ray ray,
                              out float startDistance,
                              out float endDistance)
{
    startDistance = 0.0;
    endDistance = 0.0;
    float bottom = cloudLayerBottom(inputs);
    float top = cloudLayerTop(inputs);
    if (abs(ray.direction.y) < 0.012)
    {
        return false;
    }

    float bottomDistance = (bottom - ray.origin.y) / ray.direction.y;
    float topDistance = (top - ray.origin.y) / ray.direction.y;
    startDistance = max(min(bottomDistance, topDistance), 0.0);
    endDistance = max(bottomDistance, topDistance);
    if (endDistance <= startDistance)
    {
        return false;
    }

    float thickness = max(top - bottom, 1.0);
    float grazing = 1.0 - smoothstep(
        0.05, 0.32, abs(sun_info.sun_dir.y));
    float pathCap = thickness * lerp(3.2, 7.5, grazing);
    endDistance = min(endDistance, startDistance + pathCap);
    return endDistance > startDistance;
}

Ray makeCloudShadowRay(CloudInputs inputs, float2 uv)
{
    Ray ray = generateCameraRay(
        uv, shadow_camera_info.view_proj_mat_inv);
    if (ray.direction.y < -0.001)
    {
        float top = cloudLayerTop(inputs);
        float bottom = cloudLayerBottom(inputs);
        float startHeight = top + max(2.0, (top - bottom) * 0.03);
        float distance = (startHeight - ray.origin.y) / ray.direction.y;
        ray.origin += ray.direction * distance;
    }
    ray.inverse_direction = 1.0 / ray.direction;
    return ray;
}

float computeCloudShadowTransmittance(CloudInputs inputs, Ray ray,
                                      float jitter)
{
    if (gui_input.cloud_shadows_enabled == 0u ||
        gui_input.clouds_enabled == 0u ||
        gui_input.cloud_coverage <= 0.001 ||
        gui_input.cloud_density <= 0.001)
    {
        return 1.0;
    }

    float sunFade = smoothstep(0.025, 0.13, sun_info.sun_dir.y);
    if (sunFade <= 0.001)
    {
        return 1.0;
    }

    float startDistance;
    float endDistance;
    if (!cloudShadowLayerInterval(
        inputs, ray, startDistance, endDistance))
    {
        return 1.0;
    }

    uint steps = clamp(gui_input.cloud_shadow_steps, 2u, 16u);
    float pathLength = endDistance - startDistance;
    float stepDistance = pathLength / float(steps);
    float opticalDepth = 0.0;
    float distance = startDistance + stepDistance * (0.35 + 0.30 * jitter);
    for (uint i = 0u; i < steps; ++i)
    {
        float3 worldPosition = ray.origin + ray.direction * distance;
        opticalDepth += cloudDensityLowQuality(
            inputs, worldPosition) * stepDistance;
        distance += stepDistance;
    }

    float rawTransmittance = exp(
        -opticalDepth * gui_input.cloud_absorption * 0.018);
    float strength = max(gui_input.cloud_shadow_strength, 0.0) * sunFade;
    float transmittance = lerp(1.0, rawTransmittance, strength);
    float minimumTransmittance = clamp(
        gui_input.cloud_shadow_min_transmittance, 0.0, 1.0);
    return clamp(transmittance, minimumTransmittance, 1.0);
}

[shader("compute")]
[numthreads(CLOUD_GROUP_SIZE, CLOUD_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    cloud_shadow_raw_tex.GetDimensions(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= int2(width, height)))
    {
        return;
    }

    CloudInputs inputs;
    inputs.gui = gui_input;
    inputs.sun = sun_info;
    inputs.environment = env_info;
    float2 uv = (float2(coordinate) + float2(0.5)) /
                float2(width, height);
    Ray ray = makeCloudShadowRay(inputs, uv);
    float jitter = randomFloat(
        scalar_bn,
        getBlueNoiseSeed(dispatchThreadId.xy, env_info.frame_serial_idx));
    float transmittance = computeCloudShadowTransmittance(
        inputs, ray, jitter);
    cloud_shadow_raw_tex.Store(coordinate, transmittance);
}
