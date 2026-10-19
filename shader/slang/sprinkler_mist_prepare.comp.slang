import ddgi_query;
import flora_types;
import sprinkler_animation;
import sprinkler_mist_geometry;
import sprinkler_mist_transport;
import sprinkler_spray;
import sprinkler_spray_contact;
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
[[vk::binding(3,1)]] RWStructuredBuffer<B_SprinklerWaterContact> water_contacts;
[[vk::binding(4,1)]] RWStructuredBuffer<B_SprinklerMistBounds> water_bounds;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerMist> mist_push;

groupshared float3 lowerBounds[64];
groupshared float3 upperBounds[64];
groupshared float3 waterLower[64];
groupshared float3 waterUpper[64];

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
    float3 waterLo=lo,waterHi=hi;
    // Multiple velocity bands share one instance group. Each lane accumulates
    // all of its samples before the group bounds reduction.
    for(uint index=local;index<mist_push.lobes_per_instance;index+=64u) {
        B_SprinklerMistLobe lobe=(B_SprinklerMistLobe)0;
        B_SprinklerWaterContact contact=(B_SprinklerWaterContact)0;
        float4 pose=sprinkler_instances[instance];
        float seconds=max(gui_input.world_tick_seconds,1.0/240.0);
        float pairTicks=max(floor(1.0/seconds+0.5),2.0);
        // Integer reduction preserves sub-second ages after days of uptime.
        bool rotary=gui_input.sprinkler_rotary!=0u;
        uint cycle=rotary?sprinklerRotaryTicks(seconds):uint(pairTicks)*2u;
        float tick=float(flora_growth_info.flora_tick%cycle)+(rotary?flora_growth_info.flora_tick_fraction:0.0);
        MistSpraySource source=rotary?
            mistRotarySource(index,mist_push.lobes_per_instance,gui_input.sprinkler_rotor_angle,pose.w,
                gui_input.sprinkler_rotation_speed,gui_input.sprinkler_spray_angle,
                gui_input.sprinkler_pulse_duty,gui_input.sprinkler_pulse_time):
            mistSpraySource(index,mist_push.lobes_per_instance,tick,seconds,pose.w,false);
        float age=source.age;
        float airFraction=1.0;
        if(source.emission>0.0 && mistGeometryReady()) {
            float3 position=pose.xyz+source.outlet;
            float2 wind=authoredWindEnabled() ? sampleAuthoredWind(pose.xz*256.0) : float2(0);
            wind*=0.03/(1.0+length(wind));
            float3 target=mistSprayTarget(wind,rotary);
            float3 velocity=source.velocity;
            // Trace to the end of the birth interval, not only its center: an
            // interval can straddle the air/soil boundary. Publish that boundary
            // once and let both consumers use it.
            float traceAge=rotary?age+source.width*0.5:age;
            uint steps=uint(ceil(traceAge/(rotary?0.0625:0.125)));
            float dt=traceAge/float(steps);
            bool alive=true;
            for(uint s=0;s<steps;++s) {
                float3 next,nextVelocity;
                mistFlowAdvance(position,velocity,target,dt,next,nextVelocity);
                if(rotary) {
                    float fraction;float3 normal;
                    if(mistSweepTerrain(position,next,fraction,normal)) {
                        float hitAge=(float(s)+fraction)*dt;
                        float3 hit=lerp(position,next,fraction);
                        if(dot(normal,normal)>0.0 && sprinklerArrivesInBin(hitAge,source)) {
                            contact=sprinklerWaterContact(source,hit,normal,target,seconds,hitAge);
                            float3 contactLo,contactHi;
                            sprinklerContactBounds(contact,contactLo,contactHi);
                            waterLo=min(waterLo,contactLo);waterHi=max(waterHi,contactHi);
                            airFraction=saturate((hitAge-(age-source.width*0.5))/source.width);
                            age=min(age,hitAge-0.0001);
                        } else alive=false;
                        break;
                    }
                } else if(!mistConstrainMotion(position,next,nextVelocity)) { alive=false;break; }
                position=next;velocity=nextVelocity;
            }
            if(alive) {
                // In free flight use the same exact flow, at the visible age.
                // Contact-clipped volume never moves to a separately fitted path.
                if(rotary)mistFlowAdvance(pose.xyz+source.outlet,source.velocity,target,max(0.0,age),position,velocity);
                float3 tangent=rotary?mistSprayAgeTangent(source,target,seconds):float3(1,0,0);
                lobe.tangent_rotary=float4(normalize(tangent),rotary?1.0:0.0);
                float3 radii=rotary?mistSprayRadii(source,tangent):mistFlowRadii(age,source.width,source.nozzle);
                float3 boundsRadius=mistSprayBoundsRadius(radii,lobe.tangent_rotary);
                float amplitude=mistKernelAmplitude(mistSprayVisibleAmount(source,rotary)*airFraction,radii);
                lobe.center_amplitude=float4(position,amplitude);
                lobe.radii_near_geometry=float4(radii,mistLobeNearTerrain(position,boundsRadius) ? 1.0 : 0.0);
                float3 ambient=sampleDdgiVolumeEnvironment(shading_info,position).irradiance;
                float visibility=mist_push.shadows_enabled==0u ? 1.0 :
                    sampleDirectSunShadow(pointSunShadowReceiver(position),gui_input,shadow_camera_info).combined;
                lobe.ambient_sun_visibility=float4(ambient,visibility);
                lo=min(lo,position-boundsRadius);hi=max(hi,position+boundsRadius);
            }
        }
        mist_lobes[instance*mist_push.lobes_per_instance+index]=lobe;
        water_contacts[instance*mist_push.lobes_per_instance+index]=contact;
    }
    lowerBounds[local]=lo;upperBounds[local]=hi;
    waterLower[local]=waterLo;waterUpper[local]=waterHi;
    GroupMemoryBarrierWithGroupSync();
    for(uint offset=32u;offset>0u;offset>>=1u) {
        if(local<offset) {
            lowerBounds[local]=min(lowerBounds[local],lowerBounds[local+offset]);
            upperBounds[local]=max(upperBounds[local],upperBounds[local+offset]);
            waterLower[local]=min(waterLower[local],waterLower[local+offset]);
            waterUpper[local]=max(waterUpper[local],waterUpper[local+offset]);
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if(local==0u) {
        B_SprinklerMistBounds bounds;
        bounds.lower=float4(lowerBounds[0],0);bounds.upper=float4(upperBounds[0],0);
        bounds.screen_rect=mistScreenRect(bounds.lower.xyz,bounds.upper.xyz);
        mist_bounds[instance]=bounds;
        bounds.lower=float4(waterLower[0],0);bounds.upper=float4(waterUpper[0],0);
        bounds.screen_rect=float4(0);
        water_bounds[instance]=bounds;
    }
}
