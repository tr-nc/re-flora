import ddgi_query;
import flora_types;
import projection;
import ray;
import sprinkler_air_math;
import sprinkler_mist_geometry;
import sprinkler_mist_math;
import sprinkler_mist_transport;
import sprinkler_spray;
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
[[vk::binding(11,0)]] [[vk::image_format("rg32f")]] readonly RWTexture2D<float2> sprinkler_air_optical_tex;
[[vk::binding(5,1)]] StructuredBuffer<B_SprinklerAirInfo> sprinkler_air_info;
[[vk::binding(0,1)]] StructuredBuffer<float4> sprinkler_instances;
[[vk::binding(1,1)]] StructuredBuffer<B_SprinklerMistLobe> mist_lobes;
[[vk::binding(2,1)]] StructuredBuffer<B_SprinklerMistBounds> mist_bounds;
[[vk::binding(3,1)]] StructuredBuffer<uint> mist_tiles;
[[vk::binding(4,1)]] StructuredBuffer<B_SprinklerMistLobe> droplet_lobes;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerMist> mist_push;

float mistSunVisibility(float3 position)
{
    if(mist_push.shadows_enabled==0u)return 1.0;
    return sampleDirectSunShadow(pointSunShadowReceiver(position),gui_input,shadow_camera_info).combined;
}

// The approved original appearance remains available, unchecked in the A/B UI.
void integrateLegacy(Ray ray,float distance,float3 sun,out float opticalDepth,out float3 source)
{
    opticalDepth=0.0;source=float3(0);
    float time=float(flora_growth_info.spawn_time_ms)*0.001;
    for(uint i=0;i<mist_push.count;++i) {
        float4 instance=sprinkler_instances[i];
        float2 wind=authoredWindEnabled() ? sampleAuthoredWind(instance.xz*256.0) : float2(0);
        float2 drift=wind*(0.018/(1.0+length(wind)));
        float3 center=instance.xyz+float3(drift.x,0.058,drift.y);
        float3 radii=float3(0.28,0.105,0.28);
        float2 interval;
        if(!mistRayInterval(ray.origin,ray.direction,center,radii,distance,interval))continue;
        float3 midpoint=ray.origin+ray.direction*((interval.x+interval.y)*0.5);
        float3 ambient=sampleDdgiVolumeEnvironment(shading_info,midpoint).irradiance;
        const uint STEPS=12u;
        float step=(interval.y-interval.x)/float(STEPS);
        float localDepth=0.0,litDepth=0.0;
        for(uint s=0;s<STEPS;++s) {
            float3 point=ray.origin+ray.direction*(interval.x+(float(s)+0.5)*step);
            float tau=mistDensityAt(point,instance.xyz,center,radii,time,instance.w)*step*MIST_EXTINCTION*mist_push.density;
            localDepth+=tau;litDepth+=tau*mistSunVisibility(point);
        }
        opticalDepth+=localDepth;source+=ambient*localDepth+sun*litDepth;
    }
}

void integrateFlow(Ray ray,float distance,float3 sun,uint2 pixel,
                   out float opticalDepth,out float3 source)
{
    opticalDepth=0.0;source=float3(0);
    uint columns=(mist_push.width+mist_push.tile_size-1u)/mist_push.tile_size;
    uint words=(mist_push.count+31u)/32u;
    uint tile=(pixel.y/mist_push.tile_size)*columns+pixel.x/mist_push.tile_size;
    for(uint word=0;word<words;++word) {
        uint mask=mist_tiles[tile*words+word];
        while(mask!=0u) {
            uint instance=word*32u+firstbitlow(mask);mask&=mask-1u;
            B_SprinklerMistBounds bounds=mist_bounds[instance];
            float3 center=(bounds.lower.xyz+bounds.upper.xyz)*0.5;
            float radius=length((bounds.upper.xyz-bounds.lower.xyz)*0.5);
            float2 boundsInterval;
            if(!mistRayInterval(ray.origin,ray.direction,center,float3(radius),distance,boundsInterval))continue;
            uint drops=mist_push.particles_enabled!=0u?mist_push.droplets_per_instance:0u;
            uint start=mist_push.flow_enabled!=0u && mist_push.density>0.0 && mist_push.air_enabled==0u?0u:mist_push.lobes_per_instance;
            for(uint node=start;node<mist_push.lobes_per_instance+drops;++node) {
                bool particle=node>=mist_push.lobes_per_instance;
                B_SprinklerMistLobe lobe=particle?
                    droplet_lobes[instance*mist_push.droplets_per_instance+node-mist_push.lobes_per_instance]:
                    mist_lobes[instance*mist_push.lobes_per_instance+node];
                if(lobe.center_amplitude.w<=0.0)continue;
                if(particle) {
                    // Intersect a camera-parallel square, not a spherical kernel.
                    float3 originView=mul(camera_info.view_mat,float4(ray.origin-lobe.center_amplitude.xyz,0)).xyz;
                    float3 directionView=mul(camera_info.view_mat,float4(ray.direction,0)).xyz;
                    float t;
                    if(!mistQuadHit(originView,directionView,lobe.radii_near_geometry.x,distance,t))continue;
                    if(lobe.radii_near_geometry.w>0.0) {
                        float fraction;float3 normal;
                        if(mistSweepTerrain(lobe.center_amplitude.xyz,ray.origin+ray.direction*t,fraction,normal))continue;
                    }
                    // Match the original sphere's central optical depth, uniformly.
                    float tau=1.6;
                    opticalDepth+=tau;
                    source+=tau*(lobe.ambient_sun_visibility.xyz*1.5+sun*lobe.ambient_sun_visibility.w);
                    continue;
                }
                float3 localOrigin=mistSprayToLocal(ray.origin-lobe.center_amplitude.xyz,lobe.tangent_rotary);
                float3 localDirection=mistSprayToLocal(ray.direction,lobe.tangent_rotary);
                float2 interval;
                if(!mistRayInterval(localOrigin,localDirection,float3(0),
                                    lobe.radii_near_geometry.xyz,distance,interval))continue;
                if(lobe.radii_near_geometry.w>0.0 &&
                   !mistClipLobeToTerrain(ray.origin,ray.direction,lobe.center_amplitude.xyz,interval))continue;
                float tau=mistKernelLineIntegral(localOrigin,localDirection,float3(0),
                    lobe.radii_near_geometry.xyz,interval)*lobe.center_amplitude.w*MIST_EXTINCTION*mist_push.density;
                opticalDepth+=tau;
                source+=tau*(lobe.ambient_sun_visibility.xyz+sun*lobe.ambient_sun_visibility.w);
            }
        }
    }
}

[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID)
{
    if(any(id.xy>=uint2(mist_push.width,mist_push.height)))return;
    int2 pixel=int2(id.xy);
    float2 uv=(float2(id.xy)+0.5)/float2(mist_push.width,mist_push.height);
    Ray ray=generateCameraRay(uv,camera_info.view_proj_mat_inv);
    float depth=min(gfx_depth_tex.Load(int3(pixel,0)).r,compute_depth_tex[pixel]);
    // Only camera-side air at the first glass interface; no fake refracted fog.
    if(mist_push.glass_enabled!=0u)depth=min(depth,glass_front_depth_tex.Load(int3(pixel,0)));
    float3 hit=projectScreenUvToWorld(uv,depth,camera_info.view_proj_mat_inv);
    float distance=length(hit-ray.origin);
    float3 sun=sun_info.sun_color*sun_info.sun_luminance*
        sunLuminanceFromDirection(sun_info.sun_dir,1.0)*mistPhase(dot(ray.direction,normalize(sun_info.sun_dir)));
    float opticalDepth;float3 source;
    opticalDepth=0.0;source=float3(0);
    if(mist_push.flow_enabled!=0u || mist_push.particles_enabled!=0u)
        integrateFlow(ray,distance,sun,id.xy,opticalDepth,source);
    if(mist_push.flow_enabled==0u) {
        float legacyDepth;float3 legacySource;
        integrateLegacy(ray,distance,sun,legacyDepth,legacySource);
        opticalDepth+=legacyDepth;source+=legacySource;
    }
    if(mist_push.air_enabled!=0u) {
        // Add the approximate medium and exact droplets, then composite ONCE.
        // Never blend the original mist as well, and never temporally accumulate pulses.
        float2 air=sprinkler_air_optical_tex[pixel]*mist_push.density;
        opticalDepth+=air.x;
        source+=sprinkler_air_info[0].ambient_mass.xyz*air.x+sun*air.y;
    }
    if(opticalDepth<=0.0)return;
    float4 color=composited_tex[pixel];
    composited_tex[pixel]=float4(mistComposite(color.rgb,opticalDepth,source),color.a);
}
