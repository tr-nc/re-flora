// A pose/material adapter for authored flowers. Geometry coverage and display
// stay in the existing shared model-pixel modules.
module flower_model;
import flora_types;
import flora_motion;
import flora_noise;
import flora_lighting_resources;
import vegetation_response_sample;
import model_pixel_types;
import color;
import flora_shadow;
import hash;
import tracer_types;

public struct FlowerModelPart {
    public uint4 range; // first triangle, count, shared cache source, head count
    public float4 center_radius;
    public float4 stem; // tip x/y, max bend fraction, stem/leaf triangle count
};
public struct PushConstantFlower {
    public uint3 chunk_world_offset;
    public uint species;
    public uint response_offset;
    public float time;
    public uint resolution;
    public float world_scale;
    public uint tile_first;
    public uint prepare_object;
    public uint padding;
};
[[vk::push_constant]] public ConstantBuffer<PushConstantFlower> flower_pc;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(4,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(6,0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(7,0)]] ConstantBuffer<U_WindVolumeInfo> wind_volume_info;
[[vk::binding(8,0)]] Sampler3D wind_volume_tex;
[[vk::binding(15,1)]] public StructuredBuffer<FlowerModelPart> flower_parts;
struct B_ManualFlowerInstances { Instance data[]; };
[[vk::binding(0,1)]] readonly GLSLShaderStorageBuffer<B_ManualFlowerInstances,Std430DataLayout> flora_instances;

public FlowerModelPart flowerPart(uint part) {
    return flower_parts[(flower_pc.species-MODEL_FLOWER_FIRST_SPECIES)*4u+part];
}
public uint flowerPartCount() { return flowerPart(0u).range.w; }
public FlowerModelPart flowerTilePart(uint tile) {
    return flowerPart(1u+tile%flowerPartCount());
}
public struct FlowerPlantPose {
    public ModelPixelFrame frame;
    public float3 tip_offset;
};
public FlowerPlantPose flowerPlantPose(uint plant) {
    Instance instance=flora_instances.data[floraSpeciesInstanceOffset(flower_pc.species)+plant];
    uint3 base=unpackInstanceLocalPosition(instance.packed_local_pos)+flower_pc.chunk_world_offset;
    uint3 root=base+uint3(0,1,0);
    uint seed=instanceSeed(root);
    float moisture=1.0;
    uint width,height,depth;chunk_atlas.GetDimensions(width,height,depth);
    if(all(base<uint3(width,height,depth))) {
        uint level=(chunk_atlas[base]>>4u)&3u;
        moisture=flora_growth_info.moisture_growth_factors[flower_pc.species][level];
    }
    float potential=gui_input.flora_growth_override_enabled!=0u?saturate(gui_input.flora_growth_override):moisture;
    float growth=min(float(unpackInstanceGrowthProgress(instance.packed_local_pos))/255.0,potential);
    float scale=flower_pc.world_scale*growth;
    float4 bounds=flowerPart(0u).center_radius;
    float plantHeight=(bounds.y+bounds.w)*flower_pc.world_scale*256.0;
    FloraSpawnAnimationPose spawn=sampleFloraSpawnAnimation(flora_growth_info,instance.spawn_start_ms,seed,plantHeight);
    float3 wind=vegetationResponseEnabled()
        ?sampleVegetationResponse(root,seed,flower_pc.species,plant,flower_pc.response_offset)
        :sampleWindVolume(wind_volume_info,wind_volume_tex,float3(root)/256.0,seed);
    float3 rest=floraNaturalRestBend(gui_input,seed,1.0,plantHeight);
    // Keep horizontal voxel layers horizontal. Wind transports whole cells,
    // leaf attachments and the terminal head; it never rotates/stretches cells.
    float3 up=float3(0,1,0);
    float yaw=constructFloat01(wellonsHash(seed))*6.2831853;
    float3 sideways=float3(cos(yaw),0,sin(yaw));
    ModelPixelFrame frame;
    frame.center=float3(root)/256.0+spawn.translation_voxels/256.0;
    frame.axisZ=normalize(cross(sideways,up));
    frame.axisX=normalize(cross(up,frame.axisZ));frame.axisY=up;
    frame.scale=spawn.visibility>0.0?scale:0.0;
    float3 displacement=float3(wind.x+rest.x,0,wind.z+rest.z)*growth/256.0;
    // Bounded smooth bending preserves positive overlap between adjacent cells
    // throughout the saved height range. The uploaded bound is also consumed
    // by CPU culling; it must not be independently tuned here.
    float limit=flowerPart(0u).stem.y*frame.scale*flowerPart(0u).stem.z;
    displacement*=min(1.0,limit/max(length(displacement),1e-7));
    frame.center+=displacement;
    FlowerPlantPose pose;pose.frame=frame;pose.tip_offset=displacement;return pose;
}
public ModelPixelFrame flowerPlantFrame(uint plant) { return flowerPlantPose(plant).frame; }
public float3 flowerStemWorld(FlowerPlantPose pose,float3 local,float anchorHeight) {
    float t=saturate(anchorHeight/max(flowerPart(0u).stem.y,1e-7));
    float weight=t*t*(3.0-2.0*t);
    return modelPixelWorld(pose.frame,local)+pose.tip_offset*(weight-1.0);
}
public float3 flowerNormal(ModelPixelFrame frame,float3 normal) {
    return normalize(frame.axisX*normal.x+frame.axisY*normal.y+frame.axisZ*normal.z);
}
public float3 shadeFlower(float3 srgb,float3 position,float3 normal) {
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,position,normal);
    return applyStylizedVoxelLighting(gui_input,sun_info,shading_info,srgbToLinear(srgb),shadow.weight,position,normal);
}
