// Shared native/cached adapters: finite-view orientation and live lighting.
// Cached surfaces reconstruct per-cell depth without changing the scene grid.
module model_mesh;
import model_bake_triangle;
import model_mesh_lighting;
import model_mesh_types;
import model_mesh_view;
import model_surface_cache;
import model_view_quantization;
import model_pixel_types;
import color;
import flora_shadow;
import environment_lighting;
import sunlight;
[[vk::binding(16,1)]] public StructuredBuffer<ButterflyMeshTriangle> model_mesh_triangles;
[[vk::binding(17,1)]] public StructuredBuffer<uint4> model_mesh_ranges;
public ModelMeshOutput emptyModelMeshVertex() {
    ModelMeshOutput output; output.position=float4(2,2,2,1);output.color=float4(0);return output;
}
public float3 meshPoint(ButterflyMeshTriangle t,uint corner) {
    return t.a.xyz+(corner==1u?t.e1.xyz:corner==2u?t.e2.xyz:float3(0));
}
public float3 meshNormal(ButterflyMeshTriangle t,uint corner) {
    return corner==0u?t.normal0.xyz:corner==1u?t.normal1.xyz:t.normal2.xyz;
}
public float3 meshShade(float3 linear,float3 position,float3 normal) {
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,position,normal);
    return applyStylizedVoxelLighting(gui_input,sun_info,shading_info,linear,shadow.weight,position,normal);
}
public ModelMeshOutput appleMeshVertex(uint vertex,uint source,ApplePixelVertex pose) {
    ModelPixelFrame physical;
    physical.center=pose.center;physical.scale=pose.scale;
    physical.axisX=pose.axisX;physical.axisY=pose.axisY;physical.axisZ=pose.axisZ;
    CachedModelVertex cached;
    if(cachedModelVertex(vertex,source,1u,physical,float3(0),cached)) {
        if(!cached.visible)return emptyModelMeshVertex();
        ModelMeshOutput output;output.position=cached.clip;
        output.color=float4(shadeAppleSurface(gui_input,sun_info,shading_info,shadow_camera_info,
            cached.position,cached.normal,cached.localPosition,cached.material,camera_info.pos.xyz),1);return output;
    }
    uint4 range=model_mesh_ranges[source];
    if(vertex>=range.y*3u)return emptyModelMeshVertex();
    ButterflyMeshTriangle t=model_mesh_triangles[range.x+vertex/3u];
    float3 point=meshPoint(t,vertex%3u),n=meshNormal(t,vertex%3u);
    ModelPixelFrame rendered=modelMeshViewFrame(physical,float3(0));
    float3 position=modelViewWorldPoint(rendered,point);
    float3 normal=normalize(modelViewWorldVector(rendered,n));
    uint material=uint(t.uv2.z+0.5);
    ModelMeshOutput output;output.position=mul(camera_info.view_proj_mat,float4(position,1));
    output.color=float4(shadeAppleSurface(gui_input,sun_info,shading_info,shadow_camera_info,
        position,normal,point,material,camera_info.pos.xyz),1);return output;
}
import apple_pixel_frame;
import apple_appearance;
