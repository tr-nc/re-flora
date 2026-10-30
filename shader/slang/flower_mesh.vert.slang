import flower_model;
import rooted_plant_motion;
import flora_types;
import flower_palette;
import model_mesh;
import model_mesh_geometry;
import model_surface_cache;
import model_mesh_view;
import model_cache_types;
import model_pixel_types;
import model_bake_triangle;
import model_mesh_lighting;
import model_mesh_types;
// The stable second model ID is the existing small blue Forget-me-not.
// Only its petal UV island emits; the stem and calyx keep ordinary lighting.
float3 flowerNightEmission(float3 linearColor,float2 uv) {
    if(flower_pc.species!=MODEL_FLOWER_FIRST_SPECIES+1u || uv.x>=0.74)return float3(0);
    float night=smoothstep(0.05,0.40,-sun_info.sun_dir.y);
    return linearColor*night*1.4;
}
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint head:SV_VulkanInstanceID) {
    uint count=flowerPartCount();
    FlowerModelPart part=flowerPart(1u+head%count);
    FlowerPlantPose pose=flowerPlantPose(head/count);
    ModelPixelFrame physical=pose.frame;
    bool rooted=part.socket.w>0.5;
    if(physical.scale<=1e-7)return emptyModelMeshVertex();
    CachedModelVertex cached;
    if(!rooted && cachedModelVertex(vertex,part.range.z,3u,physical,part.center_radius.xyz,cached)) {
        if(!cached.visible)return emptyModelMeshVertex();
        float3 normal=cached.normal;
        if(dot(normal,cached.position-camera_info.pos.xyz)>0)normal=-normal;
        float3 color=flowerPaletteColor(flower_pc.species-MODEL_FLOWER_FIRST_SPECIES,encodeFlowerUV(cached.uv));
        ModelMeshOutput output;output.position=cached.clip;
        output.color=float4(meshShade(color,cached.position,normal)+flowerNightEmission(color,cached.uv),1);return output;
    }
    if(vertex>=part.range.y*3u)return emptyModelMeshVertex();
    // One historical head-center pivot for its complete authored rigid range.
    ModelPixelFrame frame=rooted?physical:modelMeshViewFrame(physical,part.center_radius.xyz);
    ButterflyMeshTriangle t=model_mesh_triangles[part.range.x+vertex/3u];
    uint corner=vertex%3u;
    float3 local=meshPoint(t,corner);
    float3 position=modelPixelWorld(frame,local);
    float3 normal=flowerNormal(frame,meshNormal(t,corner));
    if(rooted) {
        position+=rootedPlantOffset(local.y,pose.height,pose.tip_offset);
        normal=rootedPlantNormal(normal,frame.axisY,local.y,pose.height,frame.scale,pose.tip_offset);
    }
    if(dot(normal,position-camera_info.pos.xyz)>0)normal=-normal;
    float2 uv=corner==0u?t.uv01.xy:corner==1u?t.uv01.zw:t.uv2.xy;
    float3 color=flowerPaletteColor(flower_pc.species-MODEL_FLOWER_FIRST_SPECIES,encodeFlowerUV(uv));
    ModelMeshOutput output;output.position=mul(camera_info.view_proj_mat,float4(position,1));
    output.color=float4(meshShade(color,position,normal)+flowerNightEmission(color,uv),1);return output;
}
