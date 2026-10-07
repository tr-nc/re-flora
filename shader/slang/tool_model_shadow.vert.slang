// The complete published world mesh casts into the ordinary scene sun depth map.
// Use the same instance and quantized world orientation as the visible model,
// not a light-facing billboard, cached front sheet, or approximate blob shadow.
import model_mesh_geometry;
import model_bake_triangle;
import model_mesh_lighting;
import model_view_quantization;
import world_model_instance;

struct WorldModelShadowOutput { float4 position:SV_Position; };
[shader("vertex")]
WorldModelShadowOutput main(uint vertex:SV_VulkanVertexID,uint instance:SV_VulkanInstanceID) {
    WorldModelInstance pose=world_model_instances[instance];
    uint4 range=model_mesh_ranges[pose.metadata.x];
    WorldModelShadowOutput output;
    output.position=float4(2,2,2,1);
    if(vertex>=range.y*3u)return output;
    ButterflyMeshTriangle triangle=model_mesh_triangles[range.x+vertex/3u];
    float3 position=modelViewWorldPoint(worldModelRenderedFrame(pose),meshPoint(triangle,vertex%3u));
    output.position=mul(shadow_camera_info.view_proj_mat,float4(position,1));
    return output;
}
