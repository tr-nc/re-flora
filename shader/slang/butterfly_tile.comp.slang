import particle_model_shading;
import butterfly_mesh_types;
[[vk::binding(74,0)]] RWStructuredBuffer<float4> butterfly_pixel_tiles;
// Compute-only write view of the same buffer. Fragment consumers remain read-only.
[[vk::binding(1,1)]] RWStructuredBuffer<ParticleModelRepairNode> particle_model_repair_output;
struct ModelPass { uint mode; uint count; uint tile_base; };
[[vk::push_constant]] ConstantBuffer<ModelPass> model_pass;

float4 endpoint(ButterflyMeshInstance instance,uint reference) {
    if ((reference&0x80000000u)!=0u) {
        float4 value=particle_model_repair_output[instance.repair.x+(reference&0x7fffffffu)].colorDepth;
        // A pruned/far-plane expression still has a usable color. Only the
        // explicit invalid-endpoint sentinel (2) invalidates descendants.
        return float4(value.rgb,value.w>1.0?1.0:0.0);
    }
    return sampleParticleModelCenter(instance,uint2(reference%instance.metadata.z,reference/instance.metadata.z));
}
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID) {
    if (model_pass.mode==0u) {
        uint index=id.z*64u+id.y*8u+id.x;
        if (index>=model_pass.count) return;
        ButterflyMeshInstance instance=butterfly_mesh_instances[index];
        // One invocation owns an instance's ordered expression graph. Endpoints
        // are original material samples or earlier expressions, including nodes
        // pruned out of the final silhouette. No GPU image readback or full leaf tile.
        for (uint i=0;i<instance.repair.y;i++) {
            uint address=instance.repair.x+i;
            ParticleModelRepairNode node=particle_model_repair_output[address];
            float4 a=endpoint(instance,node.links.y),b=endpoint(instance,node.links.z);
            node.colorDepth.rgb=lerp(a.rgb,b.rgb,asfloat(node.links.w)); // game HDR is linear
            if (a.w>=1.0 || b.w>=1.0) node.colorDepth.w=2.0; // never invent a missing endpoint
            particle_model_repair_output[address]=node;
        }
        return;
    }
    ButterflyMeshInstance instance=butterfly_mesh_instances[id.z];
    if (any(id.xy>=uint2(instance.metadata.z))) return;
    // mode 2 is a separate, bounded validation reference; never displayed.
    butterfly_pixel_tiles[butterflyTileAddress(model_pass.tile_base+id.z,id.xy)]=model_pass.mode==2u
        ? sampleParticleModelCenter(instance,id.xy) : sampleParticleModel(instance,id.xy);
}
