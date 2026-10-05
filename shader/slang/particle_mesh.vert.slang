import butterfly_mesh_types;
import model_mesh;
import leaf_lighting;
import model_bake_triangle;
import model_mesh_lighting;
import model_mesh_types;
import flora_shadow;
import environment_lighting;
import sunlight;
[[vk::binding(11,1)]] StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
float3 rotateMeshPose(float4 q,float3 p) {return p+2*cross(q.xyz,cross(q.xyz,p)+q.w*p);}
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint index:SV_VulkanInstanceID) {
    ButterflyMeshInstance instance=butterfly_mesh_instances[index];
    uint4 range=model_mesh_ranges[instance.metadata.x];
    if(vertex>=range.y*3u||instance.position_size.w<=0)return emptyModelMeshVertex();
    ButterflyMeshTriangle t=model_mesh_triangles[range.x+vertex/3u];
    float3 normal=normalize(rotateMeshPose(instance.view_orientation,meshNormal(t,vertex%3u)));
    float3 position=instance.position_size.xyz+rotateMeshPose(instance.view_orientation,meshPoint(t,vertex%3u))
        *instance.position_size.w*(1.53125/3.4);
    float3 color;
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,position,normal);
    float3 ambient=sampleDiffuseEnvironment(gui_input,shading_info,position,normal);
    if((instance.metadata.w&2u)!=0u) {
        color=shadeLeafWithEnvironment(instance.color.rgb,ambient,shadow.sun_transmittance,position,normal,camera_info.pos.xyz,sun_info,gui_input.leaf_transmission_strength);
    } else {
        float d=dot(normal,normalize(sun_info.sun_dir));
        float direct=lerp(max(0.0,d),abs(d),saturate(instance.lighting.x));
        color=applyStylizedVoxelLightingWithEnvironment(sun_info,instance.color.rgb,direct*shadow.sun_transmittance,ambient);
    }
    ModelMeshOutput output;output.position=mul(camera_info.view_proj_mat,float4(position,1));
    output.color=float4(color,instance.color.w);return output;
}
