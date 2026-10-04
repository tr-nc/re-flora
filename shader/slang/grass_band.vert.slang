import flora_types;
import flora_vertex;
import flora_lighting_cache;
import surface_flora_vertex;
import vegetation_response_sample;
import stem_band_geometry;
struct B_GrassInstances { Instance data[]; };
[[vk::binding(0,1)]] readonly GLSLShaderStorageBuffer<B_GrassInstances,Std430DataLayout> flora_instances;
struct B_GrassLighting { float4 irradiance[]; };
[[vk::binding(2,1)]] readonly GLSLShaderStorageBuffer<B_GrassLighting,Std430DataLayout> flora_lighting_cache;
struct VertexInput {
    [[vk::location(0)]] uint vertex:TEXCOORD0;
    uint instance:SV_VulkanInstanceID;
};
[shader("vertex")]
StemBandVertex main(VertexInput input) {
    StemBandVertex output;output.position=float4(2,2,2,1);output.color=float3(0);
    uint band=input.vertex>>3u;
    uint species=floraInstanceType();
    uint localIndex=input.instance-species*MAX_FLORA_INSTANCES_PER_SPECIES;
    Instance instance=flora_instances.data[input.instance];
    uint3 root=surfaceFloraStemWorldPosition(instance.packed_local_pos,floraChunkWorldOffset());
    uint seed=instanceSeed(root);
    uint3 base=unpackInstanceLocalPosition(instance.packed_local_pos);
    float competition=floraCompetitionGrowthFactor(base,species);
    float environment=floraEnvironmentGrowthFactor(base,floraChunkWorldOffset(),species);
    bool useResponse=vegetationResponseEnabled();
    float3 response=sampleVegetationResponse(root,seed,species,localIndex,floraPushConstants().response_offset);
    uint height=sampleGrassHeight(species,seed);
    if(band>=height)return output;
    uint info=lookupFloraVoxelInfo(species,int3(0,int(band),0));
    PreparedFloraVertex pose=prepareFloraVertex(int3(0,int(band),0),info,root,species,seed,
        unpackInstanceGrowthProgress(instance.packed_local_pos),competition,environment,
        instance.spawn_start_ms,useResponse,response,0.0);
    if(pose.should_trim)return output;
    // Adjacent centres define a continuous path. Halfway boundaries preserve
    // one color band per original voxel layer, including rooted and tip caps.
    float3 a=pose.voxel_position-float3(0,0.5/256.0,0);
    float3 b=pose.voxel_position+float3(0,0.5/256.0,0);
    float3 tangentA=float3(0,1,0),tangentB=float3(0,1,0);
    if(band>0u) {
        uint previous=lookupFloraVoxelInfo(species,int3(0,int(band-1u),0));
        PreparedFloraVertex p=prepareFloraVertex(int3(0,int(band-1u),0),previous,root,species,seed,
            unpackInstanceGrowthProgress(instance.packed_local_pos),competition,environment,
            instance.spawn_start_ms,useResponse,response,0.0);
        a=(p.voxel_position+pose.voxel_position)*0.5;
        tangentA=stemBandDirection(pose.voxel_position-p.voxel_position);
        tangentB=tangentA;b=pose.voxel_position+tangentB*(0.5/256.0);
    }
    if(band+1u<height) {
        uint next=lookupFloraVoxelInfo(species,int3(0,int(band+1u),0));
        PreparedFloraVertex p=prepareFloraVertex(int3(0,int(band+1u),0),next,root,species,seed,
            unpackInstanceGrowthProgress(instance.packed_local_pos),competition,environment,
            instance.spawn_start_ms,useResponse,response,0.0);
        if(!p.should_trim) {
            b=(p.voxel_position+pose.voxel_position)*0.5;
            tangentB=stemBandDirection(p.voxel_position-pose.voxel_position);
        }
    }
    float ra=0.5/256.0*stemBandTaper(gui_input.stem_band_mode,float(band)/float(height));
    float rb=0.5/256.0*stemBandTaper(gui_input.stem_band_mode,float(band+1u)/float(height));
    float3 world=stemBandPoint(a,b,tangentA,tangentB,float3(1,0,0),float2(ra),float2(rb),stemBandCorner(input.vertex));
    output.position=mul(camera_info.view_proj_mat,float4(world,1));
    float3 color=sampleFloraBaseColor(true,species,seed,int3(0,int(band),0),root,pose.color_gradient,info);
    color=applyGrassGrowthStressTint(color,true,effectiveGrowthPotential(true,competition,environment));
    float3 light=flora_lighting_cache.irradiance[floraLightingCacheIndex(floraPushConstants(),localIndex,band)].xyz;
    output.color=shadeFloraVertexWithEnvironment(color,pose.shadow_weight,light,pose.voxel_position);
    return output;
}
