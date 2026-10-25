import ddgi_query;
import hash;
import flora_types;
import sprinkler_animation;
import sprinkler_mist_geometry;
import sprinkler_mist_math;
import sprinkler_mist_transport;
import sprinkler_spray;
import sprinkler_spray_contact;
import tracer_types;
import tracer_shadowing;
import wind_field;
import vegetation_water_geometry;
import vegetation_water_types;

[[vk::binding(0,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(5,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(6,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(0,1)]] StructuredBuffer<B_SprinklerInstance> sprinkler_instances;
[[vk::binding(1,1)]] RWStructuredBuffer<B_SprinklerMistLobe> mist_lobes;
[[vk::binding(2,1)]] RWStructuredBuffer<B_SprinklerMistBounds> mist_bounds;
[[vk::binding(3,1)]] RWStructuredBuffer<B_SprinklerWaterContact> water_contacts;
[[vk::binding(4,1)]] RWStructuredBuffer<B_SprinklerMistBounds> water_bounds;
[[vk::binding(5,1)]] RWStructuredBuffer<B_SprinklerMistLobe> droplet_lobes;
[[vk::binding(6,1)]] RWStructuredBuffer<VegetationWaterLobe> vegetation_spray;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerMist> mist_push;

// Small sprinkler populations otherwise expose only two warps per instance.
// More lanes evaluate the same independent slots concurrently; only exact
// min/max bounds are reduced, so source/contact order and dose do not change.
static const uint PREPARE_LANES=256u;
groupshared float3 lowerBounds[PREPARE_LANES];
groupshared float3 upperBounds[PREPARE_LANES];
groupshared float3 waterLower[PREPARE_LANES];
groupshared float3 waterUpper[PREPARE_LANES];

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
[numthreads(PREPARE_LANES,1,1)]
void main(uint3 group:SV_GroupID,uint local:SV_GroupIndex)
{
    uint instance=group.x;
    float3 lo=float3(1.0e10),hi=float3(-1.0e10);
    float3 waterLo=lo,waterHi=hi;
    // Multiple velocity bands share one instance group. Each lane accumulates
    // all of its samples before the group bounds reduction.
    for(uint index=local;index<mist_push.lobes_per_instance+mist_push.droplets_per_instance;index+=PREPARE_LANES) {
        bool particle=index>=mist_push.lobes_per_instance;
        uint droplet=index-mist_push.lobes_per_instance;
        B_SprinklerMistLobe lobe=(B_SprinklerMistLobe)0;
        B_SprinklerWaterContact contact=(B_SprinklerWaterContact)0;
        VegetationWaterLobe plantWater=(VegetationWaterLobe)0;
        B_SprinklerInstance emitter=sprinkler_instances[instance];
        float4 pose=emitter.position_phase;
        float seconds=max(gui_input.world_tick_seconds,1.0/240.0);
        MistSpraySource source=(MistSpraySource)0;
        if(!particle) source=mistRotarySource(index,mist_push.lobes_per_instance,gui_input.sprinkler_rotor_angle,pose.w,
            gui_input.sprinkler_rotation_speed,gui_input.sprinkler_spray_angle,
            gui_input.sprinkler_pulse_duty,gui_input.sprinkler_pulse_time,gui_input.sprinkler_pulse_frequency,
            frac(gui_input.sprinkler_quadrature+floor(frac(pose.w)*64.0)/64.0),gui_input.sprinkler_size_scale,emitter.kind.x);
        else if(mist_push.particles_enabled!=0u)
            source=mistDropletSource(droplet,mist_push.droplets_per_instance,gui_input.sprinkler_droplet_time,
                gui_input.sprinkler_rotor_angle,pose.w,gui_input.sprinkler_rotation_speed,
                gui_input.sprinkler_spray_angle,gui_input.sprinkler_pulse_duty,
                gui_input.sprinkler_pulse_time,gui_input.sprinkler_pulse_frequency,gui_input.sprinkler_size_scale,emitter.kind.x);
        const bool rotary=true;
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
            float traceAge=age+source.width*0.5;
            uint steps=max(1u,uint(ceil(traceAge/0.0625)));
            float dt=traceAge/float(steps);
            bool alive=true;
            for(uint s=0;s<steps;++s) {
                float3 next,nextVelocity;
                mistFlowAdvance(position,velocity,target,dt,next,nextVelocity);
                if(rotary) {
                    float fraction;float3 normal;
                    if(mistSweepTerrain(position,next,fraction,normal)) {
                        if(particle) { alive=false;break; }
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
                float amplitude=mistKernelAmplitude(mistSprayVisibleAmount(source,rotary)*airFraction,radii);
                if(particle) {
                    float radius=lerp(0.00035,0.00075,hash11(float(droplet)+pose.w*8192.0+61.0));
                    radii=float3(radius);
                    lobe.tangent_rotary=float4(1,0,0,0);
                    amplitude=1.5/(MIST_EXTINCTION*radius);
                }
                float3 boundsRadius=mistSprayBoundsRadius(radii,lobe.tangent_rotary);
                // A rotated screen-facing square includes corners outside the old sphere.
                if(particle)boundsRadius=float3(radii.x*sqrt(2.0));
                lobe.center_amplitude=float4(position,amplitude);
                lobe.radii_near_geometry=float4(radii,mistLobeNearTerrain(position,boundsRadius) ? 1.0 : 0.0);
                if(!particle && gui_input.vegetation_spray_response!=0u)
                    plantWater=vegetationWaterLobe(position,boundsRadius,velocity,source.emission*airFraction);
                float3 ambient=sampleDdgiVolumeEnvironment(shading_info,position).irradiance;
                float visibility=mist_push.shadows_enabled==0u ? 1.0 :
                    sampleDirectSunShadow(pointSunShadowReceiver(position),gui_input,shadow_camera_info).combined;
                lobe.ambient_sun_visibility=float4(ambient,visibility);
                lo=min(lo,position-boundsRadius);hi=max(hi,position+boundsRadius);
            }
        }
        if(particle) droplet_lobes[instance*mist_push.droplets_per_instance+droplet]=lobe;
        else {
            mist_lobes[instance*mist_push.lobes_per_instance+index]=lobe;
            water_contacts[instance*mist_push.lobes_per_instance+index]=contact;
            vegetation_spray[instance*mist_push.lobes_per_instance+index]=plantWater;
        }
    }
    lowerBounds[local]=lo;upperBounds[local]=hi;
    waterLower[local]=waterLo;waterUpper[local]=waterHi;
    GroupMemoryBarrierWithGroupSync();
    for(uint offset=PREPARE_LANES/2u;offset>0u;offset>>=1u) {
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
