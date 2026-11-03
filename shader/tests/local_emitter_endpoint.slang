import local_lighting;
import voxel_types;

export __extern_cpp int main()
{
    U_LightGpu light={};
    light.kind=LOCAL_LIGHT_KIND_POINT;
    light.position=float3(419.5,148.5,701.5)/256.0;
    light.shape_params=float4(float3(1.5)/256.0,1.0);
    uint failures=0u,cases=0u;
    // Native grazing-ray reproduction: Contree rounded a source-surface hit
    // about 12 micrometres before the Gauss endpoint. A fixed 10-micrometre
    // segment trim incorrectly calls its own emissive voxel an external blocker.
    float3 endpoint=float3(1.635289,0.57421875,2.7368515);
    float3 hit=float3(1.6352899,0.57421875,2.7368393);
    cases++; if(length(endpoint-hit)<=1.0e-5) failures++;
    float3 cell=float3(418.5,147.5,700.5)/256.0;
    cases++; if(!finiteEmitterOwnsHit(light,VOXEL_TYPE_EMISSIVE,cell)) failures++;
    for(uint z=0u;z<3u;++z)for(uint y=0u;y<3u;++y)for(uint x=0u;x<3u;++x)
    {
        float3 center=light.position+(float3(x,y,z)-1.0)/256.0;
        cases++; if(!finiteEmitterOwnsHit(light,VOXEL_TYPE_EMISSIVE,center)) failures++;
        // Foreign edits inside the emitter bounds remain genuine blockers.
        for(uint type=0u;type<16u;++type) if(type!=VOXEL_TYPE_EMISSIVE)
        { cases++; if(finiteEmitterOwnsHit(light,type,center)) failures++; }
    }
    for(uint axis=0u;axis<3u;++axis)for(uint side=0u;side<2u;++side)
    {
        float3 outside=light.position; outside[axis]+=(side==0u ? -2.0 : 2.0)/256.0;
        cases++; if(finiteEmitterOwnsHit(light,VOXEL_TYPE_EMISSIVE,outside))failures++;
    }
    light.shape_params.w=0.0;
    cases++; if(finiteEmitterOwnsHit(light,VOXEL_TYPE_EMISSIVE,cell))failures++;
    printf("finite_endpoint_ownership cases=%u failures=%u\n",cases,failures);
    return failures==0u ? 0 : 1;
}
