// Cache-only particle relighting; no triangle stream or reconstruction interface.
import butterfly_mesh_types;
import model_pixel_types;
import model_pixel_object;
import model_pixel_cache;
import model_cache_types;
import leaf_lighting;
import flora_shadow;
[[vk::binding(11,1)]] public StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
public float3 rotatePose(float4 q,float3 p) { return p+2.0*cross(q.xyz,cross(q.xyz,p)+q.w*p); }
// Production uses only the shared surface; physical lighting/pose remain live.
public float4 sampleCanonicalParticle(ButterflyMeshInstance instance,uint2 pixel,uint objectIndex) {
    uint view=uint(model_object_samples[objectIndex*4u+3u].w);
    ModelBakedSurface surface=modelSurface(instance.metadata.x,view,instance.metadata.z,pixel);
    if(surface.positionDepth.w>=1.0)return float4(0,0,0,1);
    float3 normal=normalize(rotatePose(instance.view_orientation,surface.normalMaterial.xyz));
    float3 position=instance.position_size.xyz+rotatePose(instance.view_orientation,surface.positionDepth.xyz)
        *instance.position_size.w*(1.53125/3.4);
    float4 light=model_object_samples[objectIndex*4u];
    float3 color;
    if((instance.metadata.w&2u)!=0u) {
        color=shadeLeafWithEnvironment(instance.color.rgb*(surface.normalMaterial.w>0.5?0.72:1.0),
            light.rgb,light.w,position,normal,camera_info.pos.xyz,sun_info,gui_input.leaf_transmission_strength);
    } else {
        float d=dot(normal,normalize(sun_info.sun_dir));
        float direct=lerp(max(0.0,d),abs(d),saturate(instance.lighting.x));
        color=applyStylizedVoxelLightingWithEnvironment(sun_info,instance.color.rgb,direct*light.w,light.rgb);
    }
    return float4(color,surface.positionDepth.w);
}
