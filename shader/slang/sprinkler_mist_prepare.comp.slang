import ddgi_query;
import flora_types;
import sprinkler_animation;
import sprinkler_mist_geometry;
import sprinkler_mist_transport;
import tracer_types;
import tracer_shadowing;
import wind_field;

[[vk::binding(0,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(5,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(6,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(0,1)]] StructuredBuffer<float4> sprinkler_instances;
[[vk::binding(1,1)]] RWStructuredBuffer<B_SprinklerMistLobe> mist_lobes;
[[vk::binding(2,1)]] RWStructuredBuffer<B_SprinklerMistBounds> mist_bounds;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerMist> mist_push;

groupshared float3 lowerBounds[64];
groupshared float3 upperBounds[64];

float4 mistScreenRect(float3 lo,float3 hi)
{
    if(any(lo>hi))return float4(1,1,0,0);
    float2 minUv=float2(1),maxUv=float2(0);
    for(uint corner=0;corner<8;++corner) {
        float3 p=float3((corner&1u)!=0u ? hi.x : lo.x,
                       (corner&2u)!=0u ? hi.y : lo.y,
                       (corner&4u)!=0u ? hi.z : lo.z);
        float4 clip=mul(camera_info.view_proj_mat,float4(p,1));
        if(clip.w<=0.00001)return float4(0,0,1,1); // camera inside / near-plane crossing
        float2 uv=clip.xy/clip.w*0.5+0.5;
        minUv=min(minUv,uv);maxUv=max(maxUv,uv);
    }
    // One pixel guard for raster coverage / floating point projection.
    float2 guard=1.0/float2(mist_push.width,mist_push.height);
    return float4(minUv-guard,maxUv+guard);
}

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 group:SV_GroupID,uint local:SV_GroupIndex)
{
    uint instance=group.x;
    float3 lo=float3(1.0e10),hi=float3(-1.0e10);
    if(local<mist_push.lobes_per_instance) {
        B_SprinklerMistLobe lobe=(B_SprinklerMistLobe)0;
        uint ages=mist_push.lobes_per_instance/4u;
        uint nozzle=local/ages,ageIndex=local%ages;
        float width=MIST_FLOW_LIFETIME/float(ages);
        float age=(float(ageIndex)+0.5)*width;
        float4 pose=sprinkler_instances[instance];
        float seconds=max(gui_input.world_tick_seconds,1.0/240.0);
        float pairTicks=max(floor(1.0/seconds+0.5),2.0);
        // Integer reduction preserves sub-second ages after days of uptime.
        float birthTick=float(flora_growth_info.flora_tick%(uint(pairTicks)*2u))-age/seconds;
        float emission=mistPairEmission(birthTick,width/seconds,seconds,pose.w,nozzle);
        if(emission>0.0 && mistGeometryReady()) {
            uint wrappedTick=uint(floor(birthTick)-floor(floor(birthTick)/(pairTicks*2.0))*pairTicks*2.0);
            float3 position=pose.xyz+sprinklerOutlet(wrappedTick,seconds,pose.w,nozzle);
            float2 wind=authoredWindEnabled() ? sampleAuthoredWind(pose.xz*256.0) : float2(0);
            wind*=0.03/(1.0+length(wind));
            float3 target=float3(wind.x,-MIST_FLOW_SETTLING_SPEED,wind.y);
            float3 velocity=mistFlowDirection(nozzle)*0.43+float3(0,0.22,0);
            uint steps=uint(ceil(age/0.125));
            float dt=age/float(steps);
            bool alive=true;
            for(uint s=0;s<steps;++s) {
                float3 next,nextVelocity;
                mistFlowAdvance(position,velocity,target,dt,next,nextVelocity);
                if(!mistConstrainMotion(position,next,nextVelocity)) { alive=false;break; }
                position=next;velocity=nextVelocity;
            }
            if(alive) {
                float3 radii=mistFlowRadii(age,width,nozzle);
                float amplitude=mistKernelAmplitude(mistVisibleAmount(age,width,emission),radii);
                lobe.center_amplitude=float4(position,amplitude);
                lobe.radii_near_geometry=float4(radii,mistLobeNearTerrain(position,radii) ? 1.0 : 0.0);
                float3 ambient=sampleDdgiVolumeEnvironment(shading_info,position).irradiance;
                float visibility=mist_push.shadows_enabled==0u ? 1.0 :
                    sampleDirectSunShadow(pointSunShadowReceiver(position),gui_input,shadow_camera_info).combined;
                lobe.ambient_sun_visibility=float4(ambient,visibility);
                lo=position-radii;hi=position+radii;
            }
        }
        mist_lobes[instance*mist_push.lobes_per_instance+local]=lobe;
    }
    lowerBounds[local]=lo;upperBounds[local]=hi;
    GroupMemoryBarrierWithGroupSync();
    for(uint offset=32u;offset>0u;offset>>=1u) {
        if(local<offset) {
            lowerBounds[local]=min(lowerBounds[local],lowerBounds[local+offset]);
            upperBounds[local]=max(upperBounds[local],upperBounds[local+offset]);
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if(local==0u) {
        B_SprinklerMistBounds bounds;
        bounds.lower=float4(lowerBounds[0],0);bounds.upper=float4(upperBounds[0],0);
        bounds.screen_rect=mistScreenRect(bounds.lower.xyz,bounds.upper.xyz);
        mist_bounds[instance]=bounds;
    }
}
