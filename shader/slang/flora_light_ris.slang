// Included only by dedicated RIS programs: reference kernels remain untouched.
import light_reservoir;
import local_emitter;
import terrain_ray_origin;
import marching_result;
import scene_marching;
import scene_query;

struct LightProposal
{
    uint light_index;
    float inverse_probability;
};
[[vk::binding(47, 0)]] StructuredBuffer<LightProposal> local_light_proposals;
groupshared uint3 light_ris_pool[256];

// All lanes participate, INCLUDING lanes outside the partial dispatch extent.
// The workgroup shares proposals, never final irradiance or visibility.
void presampleFloraLights(uint lane, uint3 groupId, PushConstantFlora pc)
{
    uint rays=clamp(gui_input.grass_light_ris_rays,1u,8u);
    uint candidates=clamp(gui_input.grass_light_ris_candidates,1u,32u);
    uint groupSeed=lightSamplingHash(groupId.x ^ lightSamplingHash(groupId.y) ^
        lightSamplingHash(pc.instance_ty & FLORA_INSTANCE_TYPE_MASK) ^
        lightSamplingHash(pc.chunk_world_offset.x) ^ lightSamplingHash(pc.chunk_world_offset.y+17u) ^
        lightSamplingHash(pc.chunk_world_offset.z+31u));
    for(uint i=lane;i<rays*candidates;i+=32u)
    {
        uint state=lightSamplingHash(groupSeed ^ lightSamplingHash(i));
        uint slot=lightSamplingHash(state) & 4095u;
        LightProposal p=local_light_proposals[slot];
        uint tap=min(uint(lightSamplingRandom(state)*12.0),11u);
        light_ris_pool[i]=uint3(p.light_index,tap,asuint(p.inverse_probability));
    }
    GroupMemoryBarrierWithGroupSync();
}

struct FloraRisConnection
{
    float3 response;
    float3 endpoint;
};

FloraRisConnection floraRisConnection(U_LightGpu light, uint tap,
    float3 position, float3 normal)
{
    FloraRisConnection c={};
    c.endpoint=light.position;
    if(light.abi_version!=LOCAL_LIGHT_GPU_ABI_VERSION ||
        light.kind!=LOCAL_LIGHT_KIND_POINT || light.intensity<=0.0) return c;
    if(light.shape_params.w>0.5)
    {
        LocalEmitterSample s=sampleLocalEmitter(light.position,light.shape_params.xyz,
            light.range,position,normal,tap);
        c.endpoint=s.position;
        c.response=light.color*light.intensity*s.irradiance_per_intensity;
    }
    else c.response=evaluateUnshadowedPointLight(light,position,normal).irradiance;
    return c;
}

float3 floraRisVisibility(U_LocalLightInfo info, U_LightGpu light,
    float3 position, float3 normal, float3 endpoint, float offset, uint lane)
{
    float3 origin=terrainRayOriginFromPosition(position,normal,offset);
    if(light.shape_params.w>0.5)
    {
#if RE_FLORA_GLASS_TRANSPORT
        return traceVoxelGlassEmitterConnection(info,light,contree_node_data,
            contree_leaf_data,scene_tex,chunk_atlas,origin,endpoint,lane);
#else
        return traceVoxelEmitterConnection(light,contree_node_data,
            contree_leaf_data,scene_tex,origin,endpoint,lane);
#endif
    }
    // Reuse the authoritative analytic-point transport, but never the finite
    // evaluator (which would trace all twelve taps after choosing one light).
    float3 segment=endpoint-origin;
    float distance=length(segment);
    if(distance<=light.source_radius) return float3(1.0);
    float end=max(0.0,distance-light.source_radius);
#if RE_FLORA_GLASS_TRANSPORT
    SceneStraightTransport t=traceVoxelStraightTransport(contree_node_data,contree_leaf_data,
        scene_tex,chunk_atlas,origin,segment/distance,max(end,1.0e-6),
        (info.flags & LOCAL_LIGHT_FLAG_LEGACY_SAND_GLASS)!=0u,lane);
    return t.exhausted!=0u || t.terminal.kind==SCENE_QUERY_EVENT_OPAQUE
        ? float3(0.0) : t.transmittance;
#else
    MarchingResult hit=marchScene(contree_node_data,contree_leaf_data,scene_tex,
        origin,segment/distance,distance/segment,lane);
    return !hit.is_hit || hit.distance+max(offset,1.0e-5)>=end ? float3(1.0) : float3(0.0);
#endif
}

float3 evaluateFloraRis(U_LocalLightInfo info,float3 position,float3 normal,
    float offset,uint seed,uint lane)
{
    uint count=localLightCount(info);
    if(count==0u) return float3(0.0);
    uint rays=clamp(gui_input.grass_light_ris_rays,1u,8u);
    uint candidates=clamp(gui_input.grass_light_ris_candidates,1u,32u);
    float3 result=float3(0.0);
    uint state=seed;
    for(uint ray=0u;ray<rays;++ray)
    {
        LightReservoir reservoir={};
        for(uint j=0u;j<candidates;++j)
        {
            uint index=ray*candidates+j;
            uint3 proposal=light_ris_pool[index];
            if(proposal.x>=count) continue;
            U_LightGpu light=local_lights[proposal.x];
            FloraRisConnection c=floraRisConnection(light,proposal.y,position,normal);
            float invPdf=asfloat(proposal.z)*(light.shape_params.w>0.5 ? 12.0 : 1.0);
            streamLightCandidate(reservoir,index,lightSamplingTarget(c.response),invPdf,
                lightSamplingRandom(state));
        }
        float normalization=lightReservoirNormalization(reservoir,candidates);
        if(normalization<=0.0) continue;
        uint3 selected=light_ris_pool[reservoir.selected];
        U_LightGpu light=local_lights[selected.x];
        FloraRisConnection c=floraRisConnection(light,selected.y,position,normal);
        result+=c.response*normalization*floraRisVisibility(info,light,
            position,normal,c.endpoint,offset,lane);
    }
    return result/float(rays);
}
