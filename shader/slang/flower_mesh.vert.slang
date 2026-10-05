import flower_model;
import flora_types;
import flower_palette;
import model_mesh;
import model_mesh_view;
import model_cache_types;
import model_pixel_types;
import model_bake_triangle;
import model_mesh_lighting;
import model_mesh_types;
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint head:SV_VulkanInstanceID) {
    uint count=flowerPartCount();
    FlowerModelPart part=flowerPart(1u+head%count);
    ModelPixelFrame physical=flowerPlantFrame(head/count);
    if(vertex>=part.range.y*3u||physical.scale<=1e-7)return emptyModelMeshVertex();
    // One historical head-center pivot for its complete authored rigid range.
    ModelPixelFrame frame=modelMeshViewFrame(physical,part.center_radius.xyz);
    ButterflyMeshTriangle t=model_mesh_triangles[part.range.x+vertex/3u];
    uint corner=vertex%3u;
    float3 position=modelPixelWorld(frame,meshPoint(t,corner));
    float3 normal=flowerNormal(frame,meshNormal(t,corner));
    if(dot(normal,position-camera_info.pos.xyz)>0)normal=-normal;
    float2 uv=corner==0u?t.uv01.xy:corner==1u?t.uv01.zw:t.uv2.xy;
    float3 color=flowerPaletteColor(flower_pc.species-MODEL_FLOWER_FIRST_SPECIES,encodeFlowerUV(uv));
    ModelMeshOutput output;output.position=mul(camera_info.view_proj_mat,float4(position,1));
    output.color=float4(meshShade(color,position,normal),1);return output;
}
