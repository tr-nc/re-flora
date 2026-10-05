import flora_types;
import flora_vertex;
import flora_lighting_cache;
import surface_flora_vertex;
import vegetation_response_sample;
import grass_stem_geometry;
import grass_stem_proxy;
import stem_proxy_projection;
struct B_GrassInstances { Instance data[]; };
[[vk::binding(0,1)]] readonly GLSLShaderStorageBuffer<B_GrassInstances,Std430DataLayout> flora_instances;
struct B_GrassLighting { float4 irradiance[]; };
[[vk::binding(2,1)]] readonly GLSLShaderStorageBuffer<B_GrassLighting,Std430DataLayout> flora_lighting_cache;
struct VertexInput {
    [[vk::location(0)]] uint vertex:TEXCOORD0;
    uint instance:SV_VulkanInstanceID;
};
[shader("vertex")]
GrassStemProxyVertex main(VertexInput input) {
    GrassStemProxyVertex output;
    output.position=float4(2,2,2,1);output.ray=float3(0,0,1);
    output.root_count=float4(0);output.grid_center_cell=float4(0,0,0,1);
    output.grid_x=float3(1,0,0);output.grid_y=float3(0,1,0);
    for(uint i=0u;i<GRASS_STEM_MAX_POINTS;i++) {output.points[i]=float3(0);output.colors[i]=float3(0);}
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
    output.root_count.xyz=float3(root)*FLORA_SCALING_FACTOR;
    uint maxHeight=species==FLORA_SPECIES_TALL_GRASS?8u:4u;
    uint count=0u;
    for(uint i=0u;i<maxHeight;i++) {
        int3 voxel=int3(0,int(i),0);
        uint info=lookupFloraVoxelInfo(species,voxel);
        PreparedFloraVertex pose=prepareFloraVertex(voxel,info,root,species,seed,
            unpackInstanceGrowthProgress(instance.packed_local_pos),competition,environment,
            instance.spawn_start_ms,useResponse,response,0.0);
        if(pose.should_trim)break;
        output.points[i]=(pose.voxel_position-output.root_count.xyz)/FLORA_SCALING_FACTOR;
        float3 color=sampleFloraBaseColor(true,species,seed,voxel,root,pose.color_gradient,info);
        color=applyGrassGrowthStressTint(color,true,effectiveGrowthPotential(true,competition,environment));
        float3 light=flora_lighting_cache.irradiance[
            floraLightingCacheIndex(floraPushConstants(),localIndex,i)].xyz;
        output.colors[i]=shadeFloraVertexWithEnvironment(color,pose.shadow_weight,light,pose.voxel_position);
        count++;
    }
    output.root_count.w=float(count);
    if(count==0u)return output;
    float3 lo,hi;grassStemBounds(output.points,count,lo,hi);
    StemProjection projection=projectStemProxy(input.vertex,camera_info,
        output.root_count.xyz,FLORA_SCALING_FACTOR,float2(1,0),lo,hi,(lo+hi)*0.5,
        float(count),gui_input.flower_stem_sampling.z,gui_input.grass_stem_pixelization!=0u);
    output.position=projection.position;output.ray=projection.ray;
    output.grid_center_cell=float4(projection.grid.center,projection.grid.cell);
    output.grid_x=projection.grid.x;output.grid_y=projection.grid.y;
    return output;
}
