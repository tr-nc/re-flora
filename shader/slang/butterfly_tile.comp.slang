import particle_model_shading;
import butterfly_mesh_types;
import model_pixel_types;
import model_pixel_object;
[[vk::binding(13,1)]] StructuredBuffer<uint> draw_indices;
[[vk::binding(3,1)]] RWStructuredBuffer<float4> model_pixel_tiles;
struct ModelPass { uint prepare_object; uint first; };
[[vk::push_constant]] ConstantBuffer<ModelPass> model_pass;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID) {
    uint index=draw_indices[model_pass.first+id.z];
    ButterflyMeshInstance instance=butterfly_mesh_instances[index];
    if(instance.tile.y==0u)return;
    if(model_pass.prepare_object!=0u) {
        if(any(id.xy!=uint2(0)))return;
        ModelPixelFrame orientation=identityModelPixelFrame();
        orientation.axisX=rotatePose(instance.view_orientation,float3(1,0,0));
        orientation.axisY=rotatePose(instance.view_orientation,float3(0,1,0));
        orientation.axisZ=rotatePose(instance.view_orientation,float3(0,0,1));
        prepareModelObject(index,instance.position_size.xyz,orientation,false);
        return;
    }
    if(any(id.xy>=uint2(instance.metadata.z)))return;
    model_pixel_tiles[instance.tile.x+id.y*instance.metadata.z+id.x]=sampleCanonicalParticle(instance,id.xy,index);
}
