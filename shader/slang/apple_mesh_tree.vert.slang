import apple_pixel_tree_pose;
import model_mesh;
import model_mesh_types;
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint instance:SV_VulkanInstanceID) {
    return appleMeshVertex(vertex,0u,appleTreePose(instance));
}
