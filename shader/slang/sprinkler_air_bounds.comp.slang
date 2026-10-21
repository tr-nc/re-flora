import sprinkler_air_math;
import sprinkler_mist_transport;
import sprinkler_spray;

[[vk::binding(0,0)]] StructuredBuffer<B_SprinklerMistLobe> mist_lobes;
[[vk::binding(1,0)]] RWStructuredBuffer<B_SprinklerAirInfo> sprinkler_air_info;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerAir> air_push;
groupshared float3 lower[256];
groupshared float3 upper[256];
groupshared float4 ambientMass[256];

[numthreads(256,1,1)]
[shader("compute")]
void main(uint lane:SV_GroupIndex)
{
    float3 lo=float3(1e20),hi=float3(-1e20);
    float4 ambient=float4(0);
    for(uint i=lane;i<air_push.count*air_push.lobes_per_instance;i+=256u) {
        B_SprinklerMistLobe lobe=mist_lobes[i];
        if(lobe.center_amplitude.w<=0.0)continue;
        float3 r=lobe.radii_near_geometry.xyz;
        float3 extent=mistSprayBoundsRadius(r,lobe.tangent_rotary);
        lo=min(lo,lobe.center_amplitude.xyz-extent);
        hi=max(hi,lobe.center_amplitude.xyz+extent);
        float mass=lobe.center_amplitude.w*r.x*r.y*r.z;
        ambient+=float4(lobe.ambient_sun_visibility.xyz*mass,mass);
    }
    lower[lane]=lo;upper[lane]=hi;ambientMass[lane]=ambient;
    GroupMemoryBarrierWithGroupSync();
    for(uint stride=128u;stride>0u;stride>>=1u) {
        if(lane<stride) {
            lower[lane]=min(lower[lane],lower[lane+stride]);
            upper[lane]=max(upper[lane],upper[lane+stride]);
            ambientMass[lane]+=ambientMass[lane+stride];
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if(lane==0u)
        sprinkler_air_info[0]=airGridInfo(lower[0],upper[0],air_push.grid_side,ambientMass[0]);
}
