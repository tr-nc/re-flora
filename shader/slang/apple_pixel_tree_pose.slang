module apple_pixel_tree_pose;
import apple_pixel_frame;
import apple_attachment;
import flora_vertex;
import flora_types;
import tree_scene;
struct B_TreeAppleInstances {TreeLeafInstance data[];};
[[vk::binding(0,1)]] readonly GLSLShaderStorageBuffer<B_TreeAppleInstances,Std430DataLayout> tree_leaf_instances;
public ApplePixelVertex appleTreePose(uint index) {
    ApplePixelVertex output;
    TreeLeafInstance instance=tree_leaf_instances.data[index];
    uint3 worldPosition=treeLeafWorldPosition(instance.packed_local_pos,floraChunkWorldOffset());
    float4 rotation=floraAppleRotation(worldPosition,index);
    float size=float(unpackTreeFruitRadiusVoxels(instance.packed_leaf_local_pos))/2.0*gui_input.apple_size_scale;
    output.center=appleAttachedPosition(worldPosition,size,rotation,float3(0));
    output.axisX=appleAttachedVector(worldPosition,rotation,float3(1,0,0));
    output.axisY=appleAttachedVector(worldPosition,rotation,float3(0,1,0));
    output.axisZ=appleAttachedVector(worldPosition,rotation,float3(0,0,1));
    output.scale=(2.0/256.0)*float(unpackTreeFruitRadiusVoxels(instance.packed_leaf_local_pos))/2.0*gui_input.apple_size_scale;
    output.resolution=clamp(gui_input.apple_pixel_resolution,8u,64u);
    output.tileIndex=index;
    return output;
}
