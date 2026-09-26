// Particle material/pose adapter for the common model tile generator.
// Both butterfly and leaf screen fragments only read generated tiles.
import butterfly_mesh_types;
import model_pixel_grid;
import model_pixel_surface;
import flora_shadow;
import environment_lighting;
import leaf_lighting;
import tracer_types;

import model_pixel_object;
import model_pixel_projection;
[[vk::binding(72,0)]] public StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
[[vk::binding(73,0)]] public StructuredBuffer<ButterflyMeshTriangle> butterfly_mesh_triangles;
// Dedicated transient set: frame-local repair storage must not replace the
// persistent environment/camera descriptor set.
[[vk::binding(0,1)]] public StructuredBuffer<ParticleModelRepairNode> particle_model_repairs;

bool intersectTriangleBary(float3 origin, float3 direction, ButterflyMeshTriangle tri, out float t, out float2 bary) {
    return modelPixelTriangleHit(origin,direction,tri.a.xyz,tri.e1.xyz,tri.e2.xyz,t,bary);
}
bool intersectTriangle(float3 origin, float3 direction, ButterflyMeshTriangle tri, out float t) {
    float2 bary;return intersectTriangleBary(origin,direction,tri,t,bary);
}
public float3 rotatePose(float4 q,float3 p) { return p+2.0*cross(q.xyz,cross(q.xyz,p)+q.w*p); }

float3 wingLighting(ButterflyMeshInstance instance, uint hit, float3 position, float3 normal, bool thinWing, float4 objectLight) {
    float3 sunDirection=normalize(sun_info.sun_dir);
    float direct=max(0.0,dot(normal,sunDirection));
    if(modelSingleLight())return applyStylizedVoxelLightingWithEnvironment(sun_info,
        instance.color.rgb,direct*objectLight.w,objectLight.rgb);
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
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info, position,normal);
    float3 ambient=gui_input.raster_flora_ddgi_lighting!=0u
        ? sampleDiffuseEnvironment(gui_input,shading_info,position,normal)
        : LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
    return applyStylizedVoxelLightingWithEnvironment(sun_info,instance.color.rgb,direct*shadow.sun_transmittance,ambient);
}

float3 shadeParticleModelSurface(ButterflyMeshInstance instance,uint hit,float2 hitBary,float3 position,float3 worldDirection,bool leaf,float4 objectLight) {
    ButterflyMeshTriangle triangle=butterfly_mesh_triangles[hit];
    if (leaf) {
        float3 bary=float3(1.0-hitBary.x-hitBary.y,hitBary);
        float3 normal=normalize(rotatePose(instance.lighting,triangle.normal0.xyz*bary.x+triangle.normal1.xyz*bary.y+triangle.normal2.xyz*bary.z));
        float2 uv=triangle.uv01.xy*bary.x+triangle.uv01.zw*bary.y+triangle.uv2.xy*bary.z;
        float3 receiverNormal=dot(normal,worldDirection)<0.0?normal:-normal;
        if(modelSingleLight())return shadeLeafWithEnvironment(
            instance.color.rgb*(uv.x>1.5?0.72:1.0),objectLight.rgb,objectLight.w,
            position,normal,camera_info.pos.xyz,sun_info,gui_input.leaf_transmission_strength);
        StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,shadow_camera_info, position,receiverNormal);
        float3 ambient=gui_input.raster_flora_ddgi_lighting!=0u
            ? sampleDiffuseEnvironment(gui_input,shading_info,position,normal)
            : LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
        float3 base=instance.color.rgb*(uv.x>1.5?0.72:1.0);
        return shadeLeafWithEnvironment(base,ambient,shadow.sun_transmittance,position,normal,
            camera_info.pos.xyz,sun_info,gui_input.leaf_transmission_strength);
    }
    float3 normal=normalize(cross(triangle.e1.xyz,triangle.e2.xyz));
    float transmission=saturate(instance.lighting.x);
    float3 color=wingLighting(instance,hit,position,normal,false,objectLight);
    if (transmission>0.0) {
        float3 lightNormal=dot(normal,sun_info.sun_dir)>=0.0?normal:-normal;
        float3 transmitted=wingLighting(instance,hit,position,lightNormal,true,objectLight);
        color=lerp(color,transmitted,transmission);
    }
    return color;
}

float3 shadeParticleModelSurface(ButterflyMeshInstance instance,uint hit,float2 bary,float3 position,float3 direction,bool leaf) {
    return shadeParticleModelSurface(instance,hit,bary,position,direction,leaf,float4(0));
}
public float4 sampleParticleModelSurface(ButterflyMeshInstance instance,uint source,float depth) {
    ButterflyMeshTriangle triangle=butterfly_mesh_triangles[instance.metadata.x+source];
    float3 point=triangle.a.xyz+(triangle.e1.xyz+triangle.e2.xyz)/3.0;
    bool leaf=(instance.metadata.w&2u)!=0u;
    float3 position=leaf?instance.position_size.xyz+rotatePose(instance.lighting,point)*instance.position_size.w*(1.53125/3.4):point;
    float3 direction=normalize(position-camera_info.pos.xyz);
    float3 color=shadeParticleModelSurface(instance,instance.metadata.x+source,float2(1.0/3.0,1.0/3.0),position,direction,leaf);
    return float4(color,depth);
}

ModelPixelFrame particleModelFrame(ButterflyMeshInstance instance) {
    ModelPixelFrame frame=identityModelPixelFrame();
    if((instance.metadata.w&2u)!=0u) {
        frame.center=instance.position_size.xyz;
        frame.scale=instance.position_size.w*(1.53125/3.4);
        frame.axisX=rotatePose(instance.lighting,float3(1,0,0));
        frame.axisY=rotatePose(instance.lighting,float3(0,1,0));
        frame.axisZ=rotatePose(instance.lighting,float3(0,0,1));
    }
    return frame;
}
public ModelPixelHit particleModelHitAt(ButterflyMeshInstance instance,float2 pixel) {
    return modelPixelCenterHit(butterfly_mesh_triangles,instance.metadata.x,
        instance.metadata.y,particleModelFrame(instance),butterflyTileBounds(instance,camera_info),pixel,
        instance.metadata.z,camera_info);
}
public float4 shadeParticleModelHit(ButterflyMeshInstance instance,ModelPixelHit hit) {
    if(hit.triangle==0xffffffffu)return float4(0,0,0,1);
    return float4(shadeParticleModelSurface(instance,hit.triangle,hit.bary,
        hit.position,hit.direction,(instance.metadata.w&2u)!=0u),hit.depth);
}
public float4 sampleParticleModelAt(ButterflyMeshInstance instance, float2 pixel) {
    return shadeParticleModelHit(instance,particleModelHitAt(instance,pixel));
}

public float4 sampleParticleModelCenter(ButterflyMeshInstance instance, uint2 pixel) {
    return sampleParticleModelAt(instance,float2(pixel)+0.5);
}

public ModelPixelHit particleModelHit(ButterflyMeshInstance instance,uint2 pixel,uint objectIndex) {
    ModelPixelFrame frame=particleModelFrame(instance);
    if(modelDiscreteViews())return sampleOrthographicModelPixel(butterfly_mesh_triangles,
        instance.metadata.x,instance.metadata.y,frame,modelObjectView(objectIndex),instance.position_size.xyz,
        instance.position_size.w*(1.53125*0.5),pixel,instance.metadata.z);
    return sampleModelPixelGeometry(butterfly_mesh_triangles,instance.metadata.x,
        instance.metadata.y,frame,butterflyTileBounds(instance,camera_info),pixel,instance.metadata.z,camera_info);
}
public float4 shadeParticleModelHit(ButterflyMeshInstance instance,ModelPixelHit hit,uint objectIndex) {
    if(hit.triangle==0xffffffffu)return float4(0,0,0,1);
    if(modelDiscreteViews()) {
        ButterflyMeshTriangle tri=butterfly_mesh_triangles[hit.triangle];
        hit.position=modelPixelWorld(particleModelFrame(instance),tri.a.xyz+tri.e1.xyz*hit.bary.x+tri.e2.xyz*hit.bary.y);
        hit.direction=normalize(hit.position-camera_info.pos.xyz);
    }
    float4 light=float4(0);if(modelSingleLight())light=model_object_samples[objectIndex*4u];
    return float4(shadeParticleModelSurface(instance,hit.triangle,hit.bary,
        hit.position,hit.direction,(instance.metadata.w&2u)!=0u,light),hit.depth);
}
