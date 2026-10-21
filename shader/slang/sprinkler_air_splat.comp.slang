import ddgi_query;
import sprinkler_air_math;
import sprinkler_mist_transport;
import sprinkler_spray;
import sprinkler_mist_geometry;

[[vk::binding(0,1)]] StructuredBuffer<B_SprinklerMistLobe> mist_lobes;
[[vk::binding(1,1)]] StructuredBuffer<B_SprinklerAirInfo> sprinkler_air_info;
[[vk::binding(2,1)]] RWStructuredBuffer<uint> sprinkler_air_density;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerAir> air_push;

[numthreads(64,1,1)]
[shader("compute")]
void main(uint3 group:SV_GroupID,uint lane:SV_GroupIndex)
{
    B_SprinklerAirInfo info=sprinkler_air_info[0];
    if(info.lower_valid.w==0.0 || !mistGeometryReady())return;
    B_SprinklerMistLobe lobe=mist_lobes[group.y*air_push.lobes_per_instance+group.x];
    if(lobe.center_amplitude.w<=0.0)return;
    lobe=airResampleLobe(lobe,info.cell_side.x);
    float3 radius=mistSprayBoundsRadius(lobe.radii_near_geometry.xyz,lobe.tangent_rotary);
    int side=int(air_push.grid_side);
    int3 lo=clamp(int3(floor((lobe.center_amplitude.xyz-radius-info.lower_valid.xyz)/info.cell_side.xyz)),
                  int3(0),int3(side-1));
    int3 hi=clamp(int3(floor((lobe.center_amplitude.xyz+radius-info.lower_valid.xyz)/info.cell_side.xyz)),
                  int3(0),int3(side-1));
    uint3 size=uint3(hi-lo+1);
    bool nearTerrain=mistLobeNearTerrain(lobe.center_amplitude.xyz,radius);
    for(uint index=lane;index<size.x*size.y*size.z;index+=64u) {
        uint3 cell=uint3(lo)+uint3(index%size.x,(index/size.x)%size.y,index/(size.x*size.y));
        float3 point=info.lower_valid.xyz+(float3(cell)+0.5)*info.cell_side.xyz;
        float3 q=mistSprayToLocal(point-lobe.center_amplitude.xyz,lobe.tangent_rotary)/lobe.radii_near_geometry.xyz;
        float kernel=max(0.0,1.0-dot(q,q));
        uint density=airEncodeDensity(lobe.center_amplitude.w*kernel*kernel);
        if(density==0u)continue;
        // Use the authoritative live terrain, never screen-depth collision.
        // Unknown/budget-exhausted queries reject density, not admit through walls.
        float fraction;float3 normal;
        if(nearTerrain && mistSweepTerrain(lobe.center_amplitude.xyz,point,fraction,normal))continue;
        uint previous;
        InterlockedAdd(sprinkler_air_density[airCellIndex(cell,air_push.grid_side)],density,previous);
    }
}
