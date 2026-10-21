import sprinkler_spray_contact;
import sprinkler_mist_transport;
import terrain_soil;
import terrain_wetting;
import voxel_data;
import voxel_types;

[[vk::binding(1,0)]] [[vk::image_format("r8ui")]] RWTexture3D<uint> chunk_atlas;
[[vk::binding(0,1)]] StructuredBuffer<B_SprinklerWaterContact> water_contacts;
[[vk::binding(1,1)]] StructuredBuffer<B_SprinklerMistBounds> water_bounds;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerWetting> pc;

// Preparation publishes one slot per age bin, but only first-arrival bins
// contain water. Build the stable live list once per workgroup, not once per
// soil receiver. Keep source/contact storage and summation order unchanged.
groupshared uint liveContacts[256];
groupshared uint liveCount;

bool contactReachesSurface(B_SprinklerWaterContact contact,float3 receiver)
{
    // Clip finite footprints at real terrain, not the source's height or radius.
    float3 start=contact.center_flux.xyz*256.0+contact.normal.xyz*0.5;
    float3 end=receiver+contact.normal.xyz;
    MistVoxelWalk walk=mistBeginWalk(start,end);
    uint crossings=uint(ceil(dot(abs(end-start),float3(1))))+4u;
    for(uint step=0u;step<crossings;++step) {
        if(terrainSoilLoadVoxelType(chunk_atlas,walk.cell)!=VOXEL_TYPE_EMPTY)return false;
        if(!mistWalkNext(walk))return true;
    }
    return false;
}

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 id:SV_DispatchThreadID,uint local:SV_GroupIndex)
{
    B_SprinklerMistBounds bounds=water_bounds[pc.instance];
    if(any(bounds.lower.xyz>bounds.upper.xyz) || pc.amount<=0.0)return;
    for(uint n=local;n<pc.samples;n+=64u)
        liveContacts[n]=water_contacts[pc.instance*pc.samples+n].center_flux.w>0.0?1u:0u;
    GroupMemoryBarrierWithGroupSync();
    if(local==0u) {
        uint count=0u;
        for(uint n=0u;n<pc.samples;++n)if(liveContacts[n]!=0u)liveContacts[count++]=n;
        liveCount=count;
    }
    GroupMemoryBarrierWithGroupSync();
    uint width,height,depth;chunk_atlas.GetDimensions(width,height,depth);
    int3 dimensions=int3(width,height,depth);
    int3 lo=clamp(int3(floor(bounds.lower.xyz*256.0)),int3(0),dimensions);
    int3 hi=clamp(int3(ceil(bounds.upper.xyz*256.0)),int3(0),dimensions);
    int3 span=hi-lo;
    if(any(span<=int3(0)))return;
    uint count=uint(span.x*span.y*span.z);
    // Each receiver has exactly one invocation. Instances dispatch separately,
    // so overlapping sprinkler footprints never race on the packed R8 atlas.
    for(uint index=id.x;index<count;index+=pc.invocations) {
        uint3 p=uint3(lo+int3(int(index%uint(span.x)),int((index/uint(span.x))%uint(span.y)),int(index/uint(span.x*span.y))));
        uint old=chunk_atlas[p];
        if(!terrainSoilIsVoxel(voxelTypeFromAtlasData(old)))continue;
        float3 center=float3(p)+0.5;
        float flux=0.0;
        for(uint n=0u;n<liveCount;++n) {
            B_SprinklerWaterContact contact=water_contacts[pc.instance*pc.samples+liveContacts[n]];
            float density=sprinklerContactDensity(contact,center/256.0);
            if(density>0.0 && contactReachesSurface(contact,center))flux+=density;
        }
        if(flux<=0.0)continue;
        // Legacy total-input calibration, NOT a footprint radius. The normalized
        // shared contact density now determines all spatial distribution.
        const float INPUT_AREA=0.0471238898;
        float dose=pc.amount*INPUT_AREA*flux*float(VOXEL_MOISTURE_MAX);
        uint increment=wetPatchPoissonIncrement(dose,wetPatchUniform(center,pc.patch_size,pc.seed));
        uint moisture=min(VOXEL_MOISTURE_MAX,voxelMoistureFromAtlasData(old)+increment);
        if(moisture!=voxelMoistureFromAtlasData(old))
            chunk_atlas[p]=packVoxelAtlasMoisturePreserveState(old,moisture);
    }
}
