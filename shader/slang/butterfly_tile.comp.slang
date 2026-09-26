import particle_model_shading;
import butterfly_mesh_types;
import model_pixel_surface;
import model_pixel_object;
[[vk::binding(74,0)]] RWStructuredBuffer<float4> butterfly_pixel_tiles;
[[vk::binding(75,0)]] StructuredBuffer<uint> draw_indices;
[[vk::binding(3,1)]] RWStructuredBuffer<float4> model_pixel_tiles;
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
            if (node.links.y==0xffffffffu) {
                // A conservative geometry seed samples the real model shader at
                // its projected surface, not a guessed color or a missing center.
                node.colorDepth=sampleParticleModelSurface(instance,node.links.z,node.colorDepth.w);
            } else {
                float4 a=endpoint(instance,node.links.y),b=endpoint(instance,node.links.z);
                node.colorDepth.rgb=lerp(a.rgb,b.rgb,asfloat(node.links.w)); // game HDR is linear
                if (a.w>=1.0 || b.w>=1.0) node.colorDepth.w=2.0; // never invent a missing endpoint
            }
            particle_model_repair_output[address]=node;
        }
        return;
    }
    uint instanceIndex=model_pass.mode>=3u?draw_indices[model_pass.tile_base+id.z]:id.z;
    ButterflyMeshInstance instance=butterfly_mesh_instances[instanceIndex];
    if(model_pass.mode==4u) {
        if(any(id.xy!=uint2(0)) || instance.repair.w==0u)return;
        ModelPixelFrame orientation=identityModelPixelFrame();
        orientation.axisX=rotatePose(instance.view_orientation,float3(1,0,0));
        orientation.axisY=rotatePose(instance.view_orientation,float3(0,1,0));
        orientation.axisZ=rotatePose(instance.view_orientation,float3(0,0,1));
        prepareModelObject(instanceIndex,instance.position_size.xyz,orientation,false);
        return;
    }
    if (any(id.xy>=uint2(instance.metadata.z))) return;
    // mode 2 is a separate, bounded validation reference; never displayed.
    if(model_pass.mode==3u && instance.repair.w==0u)return;
    ModelPixelHit hit;
    if(model_pass.mode==2u) {
        hit=particleModelHitAt(instance,float2(id.xy)+0.5);
        // Diagnostic-only exact GPU rays let the CPU verify intersection rather
        // than guessing how much cancellation occurred during unprojection.
        butterfly_pixel_tiles[butterflyTileAddress(model_pass.tile_base*2u+id.z,id.xy)]=float4(hit.localOrigin,asfloat(hit.triangle));
        butterfly_pixel_tiles[butterflyTileAddress(model_pass.tile_base*3u+id.z,id.xy)]=float4(hit.localDirection,0);
    } else hit=particleModelHit(instance,id.xy,instanceIndex);
    // One shading site preserves bit-identical center samples in the reference
    // and covered paths, including transmission and shared object lighting.
    float4 value=shadeParticleModelHit(instance,hit,instanceIndex);
    if(model_pass.mode!=2u && instance.repair.w!=0u)
        model_pixel_tiles[instance.repair.z+id.y*instance.metadata.z+id.x]=value;
    if(model_pass.mode!=3u)
        butterfly_pixel_tiles[butterflyTileAddress(model_pass.tile_base+id.z,id.xy)]=value;
}
