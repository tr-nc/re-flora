// Audit-only descriptors/atomics/reference loops never enter the normal kernel.
[[vk::binding(13, 1)]] RWStructuredBuffer<uint> light_ris_audit;
void risAuditSum(uint index,float value)
{
    uint old; InterlockedAdd(light_ris_audit[index],0u,old);
    uint assumed;
    do {
        assumed=old;
        InterlockedCompareExchange(light_ris_audit[index],assumed,
            asuint(asfloat(assumed)+value),old);
    } while(old!=assumed);
}
void auditFloraRis(float3 environment,float3 evaluated,float3 rendered,
    PreparedFloraVertex prepared,uint lane)
{
    float3 reference=float3(0.0);
    if(!prepared.should_trim)
    {
#if RE_FLORA_GLASS_TRANSPORT
        reference=evaluateVoxelGlassVisibleLocalLightIrradiance(floraLocalLightInfo(),
            local_lights,contree_node_data,contree_leaf_data,scene_tex,chunk_atlas,
            prepared.voxel_position,prepared.shading_normal,floraTerrainRayOriginOffsetWorld(),lane);
#else
        reference=evaluateVoxelVisibleLocalLightIrradiance(floraLocalLightInfo(),
            local_lights,contree_node_data,contree_leaf_data,scene_tex,
            prepared.voxel_position,prepared.shading_normal,floraTerrainRayOriginOffsetWorld(),lane);
#endif
    }
    float3 direct=evaluated-environment;
    bool finite=all(isfinite(evaluated)) && all(isfinite(rendered)) &&
        all(isfinite(reference)) && all(direct>=float3(-1.0e-6));
    InterlockedAdd(light_ris_audit[0],1u);
    if(!finite) { InterlockedAdd(light_ris_audit[1],1u); return; }
    float error=max(abs(evaluated-rendered).x,max(abs(evaluated-rendered).y,abs(evaluated-rendered).z));
    if(error>1.0e-5+1.0e-5*max(evaluated.x,max(evaluated.y,evaluated.z)))
        InterlockedAdd(light_ris_audit[2],1u);
    InterlockedMax(light_ris_audit[11],asuint(error));
    if(any(reference>float3(0))) InterlockedAdd(light_ris_audit[3],1u);
    if(any(direct>float3(0))) InterlockedAdd(light_ris_audit[13],1u);
    for(uint channel=0u;channel<3u;++channel)
    {
        risAuditSum(4u+channel,reference[channel]);
        risAuditSum(7u+channel,direct[channel]);
    }
    risAuditSum(10u,abs(lightSamplingTarget(direct)-lightSamplingTarget(reference)));
}
