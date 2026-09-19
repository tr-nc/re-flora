import butterfly_mesh_types;
import flora_shadow;
import environment_lighting;
import tracer_types;

[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5,0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(7,0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(9,0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10,0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(72,0)]] StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
[[vk::binding(73,0)]] StructuredBuffer<ButterflyMeshTriangle> butterfly_mesh_triangles;
// RGB = linear HDR lighting; W = actual scene projected depth, 1 for a miss.
[[vk::binding(74,0)]] RWStructuredBuffer<float4> butterfly_pixel_tiles;

bool intersectTriangle(float3 origin, float3 direction, ButterflyMeshTriangle tri, out float t) {
    float3 p = cross(direction,tri.e2.xyz);
    float det = dot(tri.e1.xyz,p);
    t=0.0;
    if (abs(det)<1e-12) return false;
    float inv = 1.0/det;
    float3 s = origin-tri.a.xyz;
    float u=dot(s,p)*inv;
    float3 q=cross(s,tri.e1.xyz);
    float v=dot(direction,q)*inv;
    t=dot(tri.e2.xyz,q)*inv;
    return u>=-1e-6 && v>=-1e-6 && u+v<=1.000001 && t>0.0;
}

[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID) {
    ButterflyMeshInstance instance=butterfly_mesh_instances[id.z];
    uint resolution=instance.metadata.z;
    if (any(id.xy>=uint2(resolution))) return;
    uint address=butterflyTileAddress(id.z,id.xy);
    butterfly_pixel_tiles[address]=float4(0.0,0.0,0.0,1.0);
    float4 bounds=butterflyTileBounds(instance,camera_info);
    if (any(bounds.xy>float2(1.0)) || any(bounds.zw<float2(-1.0))) return;
    float2 ndc=lerp(bounds.xy,bounds.zw,(float2(id.xy)+0.5)/float(resolution));
    float4 nearH=mul(camera_info.view_proj_mat_inv,float4(ndc,0.0,1.0));
    float4 farH=mul(camera_info.view_proj_mat_inv,float4(ndc,1.0,1.0));
    float3 origin=nearH.xyz/nearH.w;
    float3 direction=normalize(farH.xyz/farH.w-origin);
    float closest=1e20;
    uint hit=0xffffffffu;
    for (uint i=0; i<instance.metadata.y; ++i) {
        float distance;
        if (intersectTriangle(origin,direction,butterfly_mesh_triangles[instance.metadata.x+i],distance) && distance<closest) {
            closest=distance; hit=instance.metadata.x+i;
        }
    }
    if (hit==0xffffffffu) return;
    float3 position=origin+direction*closest;
    ButterflyMeshTriangle triangle=butterfly_mesh_triangles[hit];
    float3 normal=normalize(cross(triangle.e1.xyz,triangle.e2.xyz));
    float4 clip=mul(camera_info.view_proj_mat,float4(position,1.0));
    float depth=clip.z/clip.w;
    if (depth<0.0 || depth>=1.0) return;
    float3 sunDirection=normalize(sun_info.sun_dir);
    float direct=max(0.0,dot(normal,sunDirection));
    if (instance.metadata.w!=0u && direct>0.0) {
        float3 shadowOrigin=position+normal*max(instance.position_size.w*0.0001,1e-6);
        for (uint i=0; i<instance.metadata.y; ++i) {
            uint index=instance.metadata.x+i;
            if (index==hit) continue;
            float distance;
            if (intersectTriangle(shadowOrigin,sunDirection,butterfly_mesh_triangles[index],distance)) { direct=0.0; break; }
        }
    }
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,
        shadow_map_tex_for_vsm_ping,leaf_shadow_opacity_blended_tex,leaf_shadow_mask_tex,cloud_shadow_tex,position,normal);
    float3 ambient=gui_input.raster_flora_ddgi_lighting!=0u
        ? sampleDiffuseEnvironment(gui_input,shading_info,position,normal)
        : LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
    float3 color=applyStylizedVoxelLightingWithEnvironment(sun_info,instance.color.rgb,direct*shadow.sun_transmittance,ambient);
    butterfly_pixel_tiles[address]=float4(color,depth);
}
