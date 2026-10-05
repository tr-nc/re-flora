import apple_pixel_tree_pose;
import model_mesh;
import model_mesh_types;
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint instance:SV_VulkanInstanceID) {
    return appleMeshVertex(vertex,model_mesh_ranges[64u],appleTreePose(instance));
}
