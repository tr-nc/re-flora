import ddgi_query;
import flora_types;
import projection;
import ray;
import sprinkler_mist_math;
import sunlight;
import tracer_types;
import tracer_shadowing;
import wind_field; // owns binding 4

[[vk::binding(0,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(5,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(6,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(7,0)]] Sampler2D gfx_depth_tex;
[[vk::binding(8,0)]] [[vk::image_format("r32f")]] readonly RWTexture2D<float> compute_depth_tex;
[[vk::binding(9,0)]] [[vk::image_format("rgba16f")]] RWTexture2D<float4> composited_tex;
[[vk::binding(10,0)]] Texture2D<float> glass_front_depth_tex;
[[vk::binding(0,1)]] StructuredBuffer<float4> sprinkler_instances;

public struct PushConstantSprinklerMist {
    public uint count;
    public float density;
    public uint shadows_enabled;
    public uint glass_enabled;
};
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerMist> mist_push;

float mistSunVisibility(float3 position)
{
    if(mist_push.shadows_enabled==0u)return 1.0;
    return sampleDirectSunShadow(pointSunShadowReceiver(position),
        gui_input,shadow_camera_info).combined;
}

[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID)
{
    uint width,height;
    composited_tex.GetDimensions(width,height);
    if(any(id.xy>=uint2(width,height)))return;
    int2 pixel=int2(id.xy);
    float2 uv=(float2(id.xy)+0.5)/float2(width,height);
    Ray ray=generateCameraRay(uv,camera_info.view_proj_mat_inv);
    float depth=min(gfx_depth_tex.Load(int3(pixel,0)).r,compute_depth_tex[pixel]);
    // Integrate camera-side air before the first glass interface. Refracted mist
    // transport behind glass is not approximated as an unoccluded screen overlay.
    if(mist_push.glass_enabled!=0u)depth=min(depth,glass_front_depth_tex.Load(int3(pixel,0)));
    float3 hit=projectScreenUvToWorld(uv,depth,camera_info.view_proj_mat_inv);
    float distance=length(hit-ray.origin);
    float time=float(flora_growth_info.spawn_time_ms)*0.001;
    float3 sun=sun_info.sun_color*sun_info.sun_luminance*
        sunLuminanceFromDirection(sun_info.sun_dir,1.0)*mistPhase(dot(ray.direction,normalize(sun_info.sun_dir)));
    float opticalDepth=0.0;
    float3 integratedSource=float3(0);
    for(uint i=0;i<mist_push.count;++i) {
        float4 instance=sprinkler_instances[i];
        float2 wind=authoredWindEnabled() ? sampleAuthoredWind(instance.xz*256.0) : float2(0);
        // A persistent soft volume over the watering footprint, carried gently
        // downwind. It never vanishes when the hardware swaps opposing nozzles.
        float2 drift=wind*(0.018/(1.0+length(wind)));
        float3 center=instance.xyz+float3(drift.x,0.058,drift.y);
        float3 radii=float3(0.28,0.105,0.28);
        float2 interval;
        if(!mistRayInterval(ray.origin,ray.direction,center,radii,distance,interval))continue;
        float3 midpoint=ray.origin+ray.direction*((interval.x+interval.y)*0.5);
        // Air has no surface normal. Opposing surface queries both lose their
        // support on coplanar probe layers, producing thin dark bands.
        float3 ambient=sampleDdgiVolumeEnvironment(shading_info,midpoint).irradiance;
        const uint STEPS=12u;
        float step=(interval.y-interval.x)/float(STEPS);
        float localDepth=0.0,litDepth=0.0;
        for(uint s=0;s<STEPS;++s) {
            float3 point=ray.origin+ray.direction*(interval.x+(float(s)+0.5)*step);
            float tau=mistDensityAt(point,instance.xyz,center,radii,time,instance.w)*step*MIST_EXTINCTION*mist_push.density;
            localDepth+=tau;
            litDepth+=tau*mistSunVisibility(point);
        }
        // Optically thin overlapping emitters combine commutatively; insertion
        // order cannot change their tint or create sorting seams.
        opticalDepth+=localDepth;
        integratedSource+=ambient*localDepth+sun*litDepth;
    }
    if(opticalDepth<=0.0)return;
    float4 color=composited_tex[pixel];
    composited_tex[pixel]=float4(mistComposite(color.rgb,opticalDepth,integratedSource),color.a);
}
