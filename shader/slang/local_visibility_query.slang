// Included by the opaque / Glass / band compute variants, so the original
// segment transport specialization and its descriptors remain compile-time.
import local_visibility_policy;
import local_emitter;
import terrain_ray_origin;
import tree_scene;
#if RE_FLORA_GLASS_TRANSPORT
import scene_query;
#endif

bool localVisibilityHeader(U_LocalLightInfo info, uint geometryRevision)
{
    uint count,stride; local_light_field.GetDimensions(count,stride);
    if(count<2u) return false;
    uint4 h=asuint(local_light_field[0]), g=asuint(local_light_field[1]);
    return visibilityHeaderMatches(h,g,count,localLightCount(info),
        uint2(info.live_revision_low,info.live_revision_high),geometryRevision);
}

#if RE_FLORA_LOCAL_VISIBILITY == 2
// Audit-only float sums: no float-atomic extension, quantization or clipping.
// Weigh reused light as well as counting positive connections.
void auditVisibilityEnergy(uint index,float value)
{
    uint old; InterlockedAdd(local_light_field_audit[index],0u,old);
    uint assumed;
    do {
        assumed=old;
        InterlockedCompareExchange(local_light_field_audit[index],assumed,
            asuint(asfloat(assumed)+value),old);
    } while(old!=assumed);
}
#endif

float3 originalEmitterConnection(U_LocalLightInfo info, U_LightGpu light, float3 origin, float3 source, uint lane)
{
#if RE_FLORA_GLASS_TRANSPORT
    return traceVoxelGlassEmitterConnection(info,light,contree_node_data,contree_leaf_data,
        scene_tex,chunk_atlas,origin,source,lane);
#else
    return traceVoxelEmitterConnection(light,contree_node_data,contree_leaf_data,scene_tex,origin,source,lane);
#endif
}

float3 evaluateCachedLocalVisibility(U_LocalLightInfo info, uint geometryRevision,
    float3 receiver, float3 normal, float offset, uint lane, bool audit)
{
    float3 result=float3(0.0);
    bool header=localVisibilityHeader(info,geometryRevision);
    float3 origin=terrainRayOriginFromPosition(receiver,normal,offset);
#if RE_FLORA_GLASS_TRANSPORT
    // Contree certificates describe published SURFACES. The authoritative Glass
    // query instead walks the dense medium when its biased origin is inside a
    // filled cell, even if no Contree surface lies on the segment. Such paths
    // must retain that same dense-medium transport (including interior Glass).
    bool emptyOrigin=sceneQueryVoxelTypeAtWorld(chunk_atlas,origin,256.0)==0u;
#else
    bool emptyOrigin=true;
#endif
    for(uint i=0u;i<localLightCount(info);++i)
    {
        U_LightGpu light=local_lights[i];
        if(light.abi_version!=LOCAL_LIGHT_GPU_ABI_VERSION || light.kind!=LOCAL_LIGHT_KIND_POINT || light.shape_params.w<=0.5)
        {
#if RE_FLORA_GLASS_TRANSPORT
            result+=evaluateVoxelGlassVisibleLocalLightAtIndex(info,local_lights,
                contree_node_data,contree_leaf_data,scene_tex,chunk_atlas,receiver,normal,offset,lane,i).irradiance;
#else
            result+=evaluateVoxelVisibleLocalLightAtIndex(info,local_lights,
                contree_node_data,contree_leaf_data,scene_tex,receiver,normal,offset,lane,i).irradiance;
#endif
            continue;
        }
        for(uint tap=0u;tap<LOCAL_EMITTER_SAMPLE_COUNT;++tap)
        {
            LocalEmitterSample sample=sampleLocalEmitter(light.position,light.shape_params.xyz,
                light.range,receiver,normal,tap);
            float3 energy=light.color*light.intensity*sample.irradiance_per_intensity;
            if(!any(energy>float3(0.0))) continue;
            int certificate=-1;
            if(header && emptyOrigin && dot(sample.normal,origin-sample.position)>LOCAL_VISIBILITY_EPSILON)
            {
                uint face; float forward;
                uint cell=visibilityCell(origin-sample.position,face,forward);
                uint source=localEmitterSourceId(light.position,receiver,tap);
                float4 record=local_light_field[2u+(i*LOCAL_VISIBILITY_SOURCES+source)*LOCAL_VISIBILITY_CELLS+cell];
                certificate=visibilityCertificate(record,sample.position,origin,face);
            }
            float3 transmission;
            if(certificate==1)
                transmission=treeSceneOccludes(origin,sample.position) ? float3(0.0) : float3(1.0);
            else if(certificate==0) transmission=float3(0.0);
            else transmission=originalEmitterConnection(info,light,origin,sample.position,lane);
            result+=energy*transmission;
#if RE_FLORA_LOCAL_VISIBILITY == 2
            if(audit)
            {
                float3 reference=certificate<0 ? transmission : originalEmitterConnection(info,light,origin,sample.position,lane);
                float referenceEnergy=dot(energy*reference,float3(0.2126,0.7152,0.0722));
                float sharedEnergy=dot(energy*transmission,float3(0.2126,0.7152,0.0722));
                bool finite=all(isfinite(transmission)) && all(isfinite(reference)) &&
                    isfinite(referenceEnergy) && isfinite(sharedEnergy);
                float3 difference=abs(transmission-reference);
                float error=finite ? max(difference.x,max(difference.y,difference.z)) : 3.402823e38;
                if(finite) {
                    auditVisibilityEnergy(11u,referenceEnergy);
                    if(certificate>=0) auditVisibilityEnergy(12u,sharedEnergy);
                }
                InterlockedAdd(local_light_field_audit[0],1u);
                InterlockedAdd(local_light_field_audit[certificate==1 ? 1u : (certificate==0 ? 2u : 3u)],1u);
                if(any(reference>float3(0.0))) InterlockedAdd(local_light_field_audit[4],1u);
                if(certificate==1 && any(transmission>float3(0.0))) InterlockedAdd(local_light_field_audit[5],1u);
                if(!emptyOrigin) InterlockedAdd(local_light_field_audit[8],1u);
                if(error>1.0e-4)
                {
                    InterlockedAdd(local_light_field_audit[6],1u);
                    InterlockedAdd(local_light_field_audit[certificate==1 ? 9u : 10u],1u);
                }
                InterlockedMax(local_light_field_audit[7],asuint(error));
            }
#endif
        }
    }
    return result;
}
