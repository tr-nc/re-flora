import apple_pixel_dynamic_pose;
import model_mesh;
import model_mesh_types;
[[vk::binding(4,1)]] StructuredBuffer<float> dynamic_fruit_pixel_instances;
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint instance:SV_VulkanInstanceID) {
    uint b=instance*14u;
    float3 center=float3(dynamic_fruit_pixel_instances[b],dynamic_fruit_pixel_instances[b+1],dynamic_fruit_pixel_instances[b+2]);
    float4 rotation=float4(dynamic_fruit_pixel_instances[b+7],dynamic_fruit_pixel_instances[b+8],dynamic_fruit_pixel_instances[b+9],dynamic_fruit_pixel_instances[b+10]);
    return appleMeshVertex(vertex,0u,appleDynamicPose(center,rotation,dynamic_fruit_pixel_instances[b+6],0,instance));
}
