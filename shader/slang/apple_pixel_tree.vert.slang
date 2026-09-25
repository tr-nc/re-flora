import apple_pixel_frame;
import flora_vertex;
import flora_types;
import tree_scene;

struct B_TreeAppleInstances {TreeLeafInstance data[];};
[[vk::binding(0,1)]] readonly GLSLShaderStorageBuffer<B_TreeAppleInstances,Std430DataLayout>
    tree_leaf_instances;
struct VertexInput {
    [[vk::location(0)]] uint corner:TEXCOORD0;
    uint instance_index:SV_VulkanInstanceID;
};
[shader("vertex")]
ApplePixelVertex main(VertexInput input) {
    ApplePixelVertex output;
    TreeLeafInstance instance=tree_leaf_instances.data[input.instance_index];
    uint3 worldPosition=treeLeafWorldPosition(instance.packed_local_pos,floraChunkWorldOffset());
    uint seed=instanceSeed(worldPosition);
    uint voxelInfo=lookupFloraVoxelInfo(FLORA_SPECIES_APPLE,int3(0));
    PreparedFloraVertex prepared=prepareFloraVertex(int3(0),voxelInfo,worldPosition,
        FLORA_SPECIES_APPLE,seed,INSTANCE_GROWTH_PROGRESS_MATURE,1.0,1.0,
        INSTANCE_SPAWN_INACTIVE,input.instance_index);
    output.center=prepared.anchor_position;
    output.axisX=treeAttachmentVector(worldPosition,float3(1.0,0.0,0.0));
    output.axisY=treeAttachmentVector(worldPosition,float3(0.0,1.0,0.0));
    output.axisZ=treeAttachmentVector(worldPosition,float3(0.0,0.0,1.0));
    output.scale=(2.0/256.0)*float(unpackTreeFruitRadiusVoxels(instance.packed_leaf_local_pos))/2.0;
    output.resolution=clamp(gui_input.apple_pixel_resolution,8u,64u);
    setApplePixelQuad(output,float2(input.corner&1u,(input.corner>>1u)&1u),camera_info);
    return output;
}
