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
    PreparedFloraVertex prepared,uint lane,uint3 root,uint species,uint layer,uint row,uint lightSeed)
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
    if(light_ris_audit[16]!=0u)
    {
        uint record; InterlockedAdd(light_ris_audit[12],1u,record);
        if(record>=4096u) return;
        uint base=20u+record*24u;
        light_ris_audit[base+0u]=root.x;
        light_ris_audit[base+1u]=root.y;
        light_ris_audit[base+2u]=root.z;
        light_ris_audit[base+3u]=species;
        light_ris_audit[base+4u]=layer;
        for(uint c=0u;c<3u;++c)
        {
            light_ris_audit[base+5u+c]=asuint(prepared.voxel_position[c]);
            light_ris_audit[base+8u+c]=asuint(prepared.shading_normal[c]);
            light_ris_audit[base+11u+c]=asuint(reference[c]);
            light_ris_audit[base+14u+c]=asuint(rendered[c]-environment[c]);
        }
        uint pool=0u;
        uint n=clamp(gui_input.grass_light_ris_rays,1u,8u)*
            clamp(gui_input.grass_light_ris_candidates,1u,32u);
        for(uint i=0u;i<n;++i)
        {
            uint3 proposal=floraRisProposal(lightSeed,i);
            pool=lightSamplingHash(pool^proposal.x^lightSamplingHash(proposal.y)^proposal.z);
        }
        light_ris_audit[base+17u]=pool;
        light_ris_audit[base+18u]=floraLocalLightInfo().live_revision_low;
        light_ris_audit[base+19u]=floraLocalLightInfo().live_revision_high;
        light_ris_audit[base+20u]=shading_info.ddgi_geometry_revision;
        light_ris_audit[base+21u]=asuint(error);
        light_ris_audit[base+22u]=row;
        light_ris_audit[base+23u]=asuint(floraPushConstants().time);
    }
}
