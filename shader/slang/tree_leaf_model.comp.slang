// Adapter only: GPU-owned attached poses become the SAME model instances used
// by falling leaves. Geometry caching, shading and pixel display stay shared.
import leaf_model_pose;
import flora_types;
import flora_vertex;
import leaf_lifecycle;
import butterfly_mesh_types;
struct B_TreeLeafInstances { TreeLeafInstance data[]; };
[[vk::binding(0,1)]] readonly GLSLShaderStorageBuffer<B_TreeLeafInstances,Std430DataLayout> tree_leaf_instances;
[[vk::binding(1,1)]] StructuredBuffer<uint> leaf_model_sources;
[[vk::binding(2,1)]] RWStructuredBuffer<ButterflyMeshInstance> leaf_model_instances;
[[vk::binding(6,1)]] RWStructuredBuffer<uint> leaf_model_draw_indices;
[numthreads(64,1,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    uint count=floraLightingCacheInstanceCount(floraPushConstants());
    if(id.x>=count)return;
    uint index=leaf_model_sources[id.x];
    TreeLeafInstance source=tree_leaf_instances.data[index];
    LeafModelPose pose=attachedLeafModelPose(
        treeLeafWorldPosition(source.packed_local_pos,floraChunkWorldOffset()),
        source.packed_leaf_local_pos,index,treeLeafGrowth(index));
    uint n=clamp(gui_input.falling_leaf_pixel_resolution,8u,64u);
    ButterflyMeshInstance instance=(ButterflyMeshInstance)0;
    instance.position_size=float4(pose.center,pose.size*clamp(gui_input.falling_leaf_size_scale,0.25,4.0));
    instance.color=pose.color;
    instance.metadata=uint4(0,32,n,2); // shared leaf material
    instance.lighting=pose.rotation;
    instance.view_orientation=pose.rotation;
    instance.cache=uint4(pose.seed%64u,0,0,0);
    float4 bounds=butterflyTileBounds(instance,camera_info);
    bool visible=pose.size>0.0 && pose.color.a>0.0 &&
        !(bounds.x>1.0 || bounds.y>1.0 || bounds.z< -1.0 || bounds.w< -1.0);
    instance.repair=uint4(0,0,id.x*n*n,visible?1u:0u);
    leaf_model_instances[id.x]=instance;
    leaf_model_draw_indices[id.x]=id.x;
}
