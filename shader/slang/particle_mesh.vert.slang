import butterfly_mesh_types;
import flower_attachment;
import model_mesh;
import model_mesh_geometry;
import model_mesh_view;
import model_surface_cache;
import model_view_quantization;
import model_pixel_types;
import model_bake_triangle;
import model_mesh_lighting;
import model_mesh_types;
import flora_shadow;
import environment_lighting;
import sunlight;
import butterfly_color;
import firefly_material;
[[vk::binding(11,1)]] StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
float3 rotateMeshPose(float4 q,float3 p) {return p+2*cross(q.xyz,cross(q.xyz,p)+q.w*p);}
[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint index:SV_VulkanInstanceID) {
    ButterflyMeshInstance instance=butterfly_mesh_instances[index];
    if(instance.position_size.w<=0)return emptyModelMeshVertex();
    ModelPixelFrame physical;
    physical.center=instance.position_size.xyz;
    physical.scale=instance.position_size.w*(1.53125/3.4);
    physical.axisX=rotateMeshPose(instance.view_orientation,float3(1,0,0));
    physical.axisY=rotateMeshPose(instance.view_orientation,float3(0,1,0));
    physical.axisZ=rotateMeshPose(instance.view_orientation,float3(0,0,1));
    if(!attachFlower(instance.flower_attachment,physical.scale,physical))return emptyModelMeshVertex();
    float3 normal,position;float4 clip;uint material=0u;
    CachedModelVertex cached;
    if(cachedModelVertex(vertex,instance.metadata.x,2u,physical,float3(0),cached)) {
        if(!cached.visible)return emptyModelMeshVertex();
        normal=cached.normal;position=cached.position;clip=cached.clip;material=cached.material;
    } else {
        uint4 range=model_mesh_ranges[instance.metadata.x];
        if(vertex>=range.y*3u)return emptyModelMeshVertex();
        ButterflyMeshTriangle t=model_mesh_triangles[range.x+vertex/3u];
        material=uint(t.uv2.z+0.5);
        ModelPixelFrame rendered=modelMeshViewFrame(physical,float3(0));
        normal=normalize(modelViewWorldVector(rendered,meshNormal(t,vertex%3u)));
        position=modelViewWorldPoint(rendered,meshPoint(t,vertex%3u));
        clip=mul(camera_info.view_proj_mat,float4(position,1));
    }
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,position,normal);
    float3 ambient=sampleDiffuseEnvironment(gui_input,shading_info,position,normal);
    float d=dot(normal,normalize(sun_info.sun_dir));
    float direct=lerp(max(0.0,d),abs(d),saturate(instance.lighting.x));
    bool firefly=(instance.metadata.w&4u)!=0u;
    float3 pigment=firefly?fireflyPigment(material):instance.color.rgb;
    float3 color=applyStylizedVoxelLightingWithEnvironment(sun_info,pigment,direct*shadow.sun_transmittance,ambient);
    color=firefly?color+fireflyEmission(material,instance.lighting.y):readableButterflyColor(color,pigment,sun_info.sun_dir.y);
    ModelMeshOutput output;output.position=clip;output.color=float4(color,instance.color.w);return output;
}
