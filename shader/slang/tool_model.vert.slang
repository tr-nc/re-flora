// Material/placement adapter only: geometry, view selection, pixel cells and
// hardware depth are the same shared modules used by apples and flower heads.
import model_mesh;
import model_mesh_geometry;
import model_bake_triangle;
import model_mesh_view;
import model_view_quantization;
import model_mesh_lighting;
import model_mesh_types;
import model_surface_cache;
import model_pixel_types;
import world_model_instance;
[[vk::binding(18,1)]] StructuredBuffer<float4> tool_model_colors;
float3 toolColor(WorldModelInstance pose,uint material,float2 uv) {
    if(material<4u)return tool_model_colors[pose.metadata.y+material].xyz;
    // Opaque RGB, Nearest, no mipmap/interpolation. CanvasTexture's V flip is
    // adapted here once; sampling never treats the sticker as alpha geometry.
    uint2 size=pose.metadata.zw;
    uint2 texel=min(uint2(saturate(float2(uv.x,1.0-uv.y))*float2(size)),size-1u);
    return tool_model_colors[pose.metadata.y+4u+texel.y*size.x+texel.x].xyz;
}
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint instance:SV_VulkanInstanceID) {
    WorldModelInstance pose=world_model_instances[instance];
    ModelPixelFrame physical=worldModelPhysicalFrame(pose);
    CachedModelVertex cached;
    if(cachedModelVertex(vertex,pose.metadata.x,4u,physical,pose.mouth.xyz,cached)) {
        if(!cached.visible)return emptyModelMeshVertex();
        ModelMeshOutput output;output.position=cached.clip;
        output.color=float4(meshShade(toolColor(pose,cached.material,cached.uv),cached.position,cached.normal),1);
        return output;
    }
    uint4 range=model_mesh_ranges[pose.metadata.x];
    if(vertex>=range.y*3u)return emptyModelMeshVertex();
    ButterflyMeshTriangle t=model_mesh_triangles[range.x+vertex/3u];
    uint corner=vertex%3u;
    ModelPixelFrame rendered=worldModelRenderedFrame(pose);
    float3 position=modelViewWorldPoint(rendered,meshPoint(t,corner));
    float3 normal=normalize(modelViewWorldVector(rendered,meshNormal(t,corner)));
    float2 uv=corner==0u?t.uv01.xy:corner==1u?t.uv01.zw:t.uv2.xy;
    ModelMeshOutput output;output.position=mul(camera_info.view_proj_mat,float4(position,1));
    output.color=float4(meshShade(toolColor(pose,uint(t.uv2.z+.5),uv),position,normal),1);return output;
}
