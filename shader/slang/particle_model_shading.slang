// Shared model ray/sample implementation. Cached butterfly tiles and snapped
// leaf fragments use the same camera, coverage, actual hit depth and sun inputs.
import butterfly_mesh_types;
import flora_shadow;
import environment_lighting;
import leaf_lighting;
import tracer_types;

[[vk::binding(0,0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] public ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] public ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] public ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5,0)]] public Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(7,0)]] public Sampler2D cloud_shadow_tex;
[[vk::binding(9,0)]] public Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10,0)]] public Sampler2D leaf_shadow_mask_tex;
[[vk::binding(72,0)]] public StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
[[vk::binding(73,0)]] public StructuredBuffer<ButterflyMeshTriangle> butterfly_mesh_triangles;

bool intersectTriangleBary(float3 origin, float3 direction, ButterflyMeshTriangle tri, out float t, out float2 bary) {
    float3 p=cross(direction,tri.e2.xyz);
    float det=dot(tri.e1.xyz,p);t=0.0;bary=0.0;
    if (abs(det)<1e-12) return false;
    float inv=1.0/det;float3 s=origin-tri.a.xyz;
    float u=dot(s,p)*inv;float3 q=cross(s,tri.e1.xyz);float v=dot(direction,q)*inv;
    t=dot(tri.e2.xyz,q)*inv;bary=float2(u,v);
    return u>=-1e-6 && v>=-1e-6 && u+v<=1.000001 && t>0.0;
}
bool intersectTriangle(float3 origin, float3 direction, ButterflyMeshTriangle tri, out float t) {
    float2 bary;return intersectTriangleBary(origin,direction,tri,t,bary);
}
float3 rotatePose(float4 q,float3 p) { return p+2.0*cross(q.xyz,cross(q.xyz,p)+q.w*p); }

float3 wingLighting(ButterflyMeshInstance instance, uint hit, float3 position, float3 normal, bool thinWing) {
    float3 sunDirection=normalize(sun_info.sun_dir);
    float direct=max(0.0,dot(normal,sunDirection));
    if ((instance.metadata.w&1u)!=0u && direct>0.0) {
        float3 shadowOrigin=position+normal*max(instance.position_size.w*0.0001,1e-6);
        for (uint i=0; i<instance.metadata.y; ++i) {
            uint index=instance.metadata.x+i;
            if (index==hit) continue;
            if (thinWing && butterfly_mesh_triangles[index].e1.w==butterfly_mesh_triangles[hit].e1.w) continue;
            float distance;
            if (intersectTriangle(shadowOrigin,sunDirection,butterfly_mesh_triangles[index],distance)) { direct=0.0; break; }
        }
    }
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,
        shadow_map_tex_for_vsm_ping,leaf_shadow_opacity_blended_tex,leaf_shadow_mask_tex,cloud_shadow_tex,position,normal);
    float3 ambient=gui_input.raster_flora_ddgi_lighting!=0u
        ? sampleDiffuseEnvironment(gui_input,shading_info,position,normal)
        : LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
    return applyStylizedVoxelLightingWithEnvironment(sun_info,instance.color.rgb,direct*shadow.sun_transmittance,ambient);
}

public float4 sampleParticleModel(ButterflyMeshInstance instance, uint2 pixel) {
    const float4 miss=float4(0.0,0.0,0.0,1.0);
    float4 bounds=butterflyTileBounds(instance,camera_info);
    if (any(bounds.xy>float2(1.0)) || any(bounds.zw<float2(-1.0))) return miss;
    float2 ndc=lerp(bounds.xy,bounds.zw,(float2(pixel)+0.5)/float(instance.metadata.z));
    float4 nearH=mul(camera_info.view_proj_mat_inv,float4(ndc,0.0,1.0));
    float4 farH=mul(camera_info.view_proj_mat_inv,float4(ndc,1.0,1.0));
    float3 worldOrigin=nearH.xyz/nearH.w;
    float3 worldDirection=normalize(farH.xyz/farH.w-worldOrigin);
    float3 origin=worldOrigin,direction=worldDirection;
    bool leaf=(instance.metadata.w&2u)!=0u;
    float scale=instance.position_size.w*(1.53125/3.4);
    if (leaf) {
        float4 inversePose=float4(-instance.lighting.xyz,instance.lighting.w);
        origin=rotatePose(inversePose,worldOrigin-instance.position_size.xyz)/scale;
        direction=rotatePose(inversePose,worldDirection);
    }
    float closest=1e20;uint hit=0xffffffffu;float2 hitBary=0.0;
    for (uint i=0; i<instance.metadata.y; ++i) {
        float distance;float2 bary;
        if (intersectTriangleBary(origin,direction,butterfly_mesh_triangles[instance.metadata.x+i],distance,bary) && distance<closest) {
            closest=distance;hit=instance.metadata.x+i;hitBary=bary;
        }
    }
    if (hit==0xffffffffu) return miss;
    float3 position=worldOrigin+worldDirection*closest*(leaf?scale:1.0);
    ButterflyMeshTriangle triangle=butterfly_mesh_triangles[hit];
    float4 clip=mul(camera_info.view_proj_mat,float4(position,1.0));
    float depth=clip.z/clip.w;
    if (depth<0.0 || depth>=1.0) return miss;
    float3 color;
    if (leaf) {
        float3 bary=float3(1.0-hitBary.x-hitBary.y,hitBary);
        float3 normal=normalize(rotatePose(instance.lighting,triangle.normal0.xyz*bary.x+triangle.normal1.xyz*bary.y+triangle.normal2.xyz*bary.z));
        float2 uv=triangle.uv01.xy*bary.x+triangle.uv01.zw*bary.y+triangle.uv2.xy*bary.z;
        float3 receiverNormal=dot(normal,worldDirection)<0.0?normal:-normal;
        StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info,
            shadow_map_tex_for_vsm_ping,leaf_shadow_opacity_blended_tex,leaf_shadow_mask_tex,cloud_shadow_tex,position,receiverNormal);
        float3 ambient=gui_input.raster_flora_ddgi_lighting!=0u
            ? sampleDiffuseEnvironment(gui_input,shading_info,position,normal)
            : LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
        // Retain the game's leaf palette/transmission; geometry, UVs and normals
        // are authored in the shared GLB. Browser studio lights are not imported.
        float3 base=instance.color.rgb*(uv.x>1.5?0.72:1.0);
        color=shadeLeafWithEnvironment(base,ambient,shadow.sun_transmittance,position,normal,
            camera_info.pos.xyz,sun_info,gui_input.leaf_transmission_strength);
    } else {
        float3 normal=normalize(cross(triangle.e1.xyz,triangle.e2.xyz));
        float transmission=saturate(instance.lighting.x);
        color=wingLighting(instance,hit,position,normal,false);
        if (transmission>0.0) {
            float3 lightNormal=dot(normal,sun_info.sun_dir)>=0.0?normal:-normal;
            float3 transmitted=wingLighting(instance,hit,position,lightNormal,true);
            color=lerp(color,transmitted,transmission);
        }
    }
    return float4(color,depth);
}
