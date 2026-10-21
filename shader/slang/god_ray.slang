import composition_types;
import noise_tex;
import projection;
import ray;
import tracer_types;

#ifdef RE_FLORA_SPRINKLER_AIR
import sprinkler_air_math;
import sprinkler_air_sample;
import sprinkler_mist_math;
[[vk::binding(8,0)]] [[vk::image_format("rg32f")]]
WTexture2D<float2> sprinkler_air_optical_tex;
[[vk::binding(9,0)]] Texture2D<float> glass_front_depth_tex;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerAir> air_push;
#endif

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
WTexture2D<float> god_ray_raw_tex;

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

float sunlightVisibility(float3 position)
{
    float4 lightSpace=mul(shadow_camera_info.view_proj_mat,float4(position,1));
    float3 ndc=lightSpace.xyz/lightSpace.w;
    float2 uv=ndc.xy*0.5+0.5;
    return shadow_map_tex.SampleLevel(uv,0.0).r>ndc.z ? 1.0 : 0.0;
}

[shader("compute")]
[numthreads(GOD_RAY_GROUP_SIZE, GOD_RAY_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint outputWidth;
    uint outputHeight;
    god_ray_raw_tex.GetDimensions(outputWidth, outputHeight);
    int2 outputCoordinate = int2(dispatchThreadId.xy);
    if (any(outputCoordinate >= int2(outputWidth, outputHeight)))
    {
        return;
    }

    float2 screenUv = (float2(outputCoordinate) + float2(0.5)) /
                      float2(outputWidth, outputHeight);
    uint depthWidth;
    uint depthHeight;
    gfx_depth_tex.GetDimensions(depthWidth, depthHeight);
    int2 depthCoordinate = int2(clamp(
        screenUv * float2(depthWidth, depthHeight), float2(0.0),
        float2(int2(depthWidth, depthHeight) - int2(1))));
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    float depth = getDepth(depthCoordinate);

    float3 nearPoint = projectScreenUvToWorld(
        screenUv, 0.0, camera_info.view_proj_mat_inv);
    float3 hitFarPoint = projectScreenUvToWorld(
        screenUv, depth, camera_info.view_proj_mat_inv);
    float realDepth = length(hitFarPoint - nearPoint);
    float usedDepth = min(realDepth, god_ray_info.max_depth);
    float stepSize = usedDepth / float(god_ray_info.max_checks);

    int3 seed = getBlueNoiseSeed(
        uint2(depthCoordinate), env_info.frame_serial_idx);
    float randomValue = randomFloat(scalar_bn, seed);
    float shadowVisibility = 0.0;

#ifdef RE_FLORA_SPRINKLER_AIR
    // The global atmosphere retains its original sampling and temporal path.
    if(air_push.background_enabled!=0u)
#endif
    for (uint i = 0u; i < god_ray_info.max_checks; ++i)
    {
        float currentDepth = (randomValue + float(i)) * stepSize;
        if (currentDepth >= usedDepth)
        {
            break;
        }

        float4 lookupPosition = float4(
            ray.origin + ray.direction * currentDepth, 1.0);
        if (sunlightVisibility(lookupPosition.xyz)>0.0)
        {
            float mistWeight = exp(-lookupPosition.y * 2.0);
            shadowVisibility += mistWeight * stepSize;
        }
    }

    god_ray_raw_tex.Store(outputCoordinate, shadowVisibility);
#ifdef RE_FLORA_SPRINKLER_AIR
    float airDistance=realDepth;
    if(air_push.glass_enabled!=0u) {
        float glassDepth=glass_front_depth_tex.Load(int3(depthCoordinate,0));
        float3 glassHit=projectScreenUvToWorld(screenUv,min(depth,glassDepth),camera_info.view_proj_mat_inv);
        airDistance=length(glassHit-ray.origin);
    }
    B_SprinklerAirInfo info=airCurrentInfo();
    float2 interval;
    float2 optical=float2(0);
    if(airRayInterval(ray.origin,ray.direction,airDistance,info,interval)) {
        // Local support, not max_depth: distant sprays must not disappear just
        // because the artistic global-atmosphere length is short. Fixed midpoints
        // avoid frame-time noise / ghost trails on the 5–20 Hz valve.
        uint steps=min(96u,max(1u,uint(ceil((interval.y-interval.x)/info.cell_side.x))));
        float ds=(interval.y-interval.x)/float(steps);
        for(uint i=0u;i<steps;++i) {
            float3 p=ray.origin+ray.direction*(interval.x+(float(i)+0.5)*ds);
            float tau=airSampleDensity(p,info)*ds*MIST_EXTINCTION;
            if(tau<=0.0)continue;
            float lit=air_push.shadows_enabled!=0u ? sunlightVisibility(p) : 1.0;
            optical+=float2(tau,tau*lit);
        }
    }
    // Written for every in-range pixel, including empty fields / old coverage.
    sprinkler_air_optical_tex.Store(outputCoordinate,optical);
#endif
}
