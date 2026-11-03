import local_visibility_policy;
import local_emitter;

uint nextRandom(inout uint state) { state=state*1664525u+1013904223u; return state; }
float random01(inout uint state) { return float(nextRandom(state)&65535u)/65535.0; }

export __extern_cpp int main()
{
    uint failures=0u, cases=0u, state=914713u;
    uint4 header=uint4(LOCAL_VISIBILITY_ABI,8u,19u,2u);
    uint4 geometry=uint4(41u,32u,1u,0u);
    uint records=2u+8u*LOCAL_VISIBILITY_SOURCES*LOCAL_VISIBILITY_CELLS;
    cases++; if(!visibilityHeaderMatches(header,geometry,records,8u,uint2(19,2),41u))failures++;
    cases++; if(visibilityHeaderMatches(header,geometry,records,8u,uint2(20,2),41u))failures++;
    cases++; if(visibilityHeaderMatches(header,geometry,records,8u,uint2(19,3),41u))failures++;
    cases++; if(visibilityHeaderMatches(header,geometry,records,8u,uint2(19,2),42u))failures++;
    cases++; if(visibilityHeaderMatches(header,geometry,records-1u,8u,uint2(19,2),41u))failures++;
    geometry.z=0u; // partial rebuild: exact fallback, never read mixed records
    cases++; if(visibilityHeaderMatches(header,geometry,records,8u,uint2(19,2),41u))failures++;
    geometry.z=1u;
    cases++; if(visibilityHeaderMatches(header,geometry,records,9u,uint2(19,2),41u))failures++;
    cases++; if(visibilityHeaderMatches(header,geometry,0xffffffffu,0xffffffffu,uint2(19,2),41u))failures++;
    // If any tested ray hits, cone rejection must NEVER certify empty space.
    // Exercise all cube faces, seams, sources inside/outside boxes, tiny voxels,
    // coarse nodes and nearly tangent rays using the production query interface.
    for(uint face=0u;face<6u;++face) for(uint trial=0u;trial<400u;++trial)
    {
        uint cell=face*32u*32u+(nextRandom(state)%1024u);
        uint decoded; float2 lo,hi;
        visibilityCellSlopes(cell,decoded,lo,hi);
        float3 origin=float3(.4,.6,.8);
        float t=.001+random01(state)*1.5;
        float2 s=lerp(lo,hi,float2(random01(state),random01(state)));
        float3 center=origin+visibilityFaceVector(face,s)*t;
        float size=trial%3u==0u ? 1.0/256.0 : .01+random01(state)*.25;
        float3 lower=center-size*.5,upper=center+size*.5;
        float depth;
        bool possible=visibilityConeMayIntersect(origin,face,lo,hi,lower,upper,2.0,depth);
        for(uint v=0u;v<3u;++v)for(uint u=0u;u<3u;++u)
        {
            float3 endpoint=origin+visibilityFaceVector(face,lerp(lo,hi,float2(u,v)*.5))*2.0;
            float entry,exit;
            bool hit=visibilitySegmentBox(origin,endpoint,lower,upper,entry,exit);
            if(hit && (!possible || depth>entry*2.0+1.0e-5)) failures++;
            cases++;
        }
        float forward; uint selected;
        visibilityCell(center-origin,selected,forward);
        if(selected!=face && all(abs(s)<float2(.9999))) failures++;
    }
    // A thin off-centre blocker must not disappear just because a cell-centre
    // shadow ray misses it. The entire angular interval, not its centre, owns
    // the empty-space certificate.
    float nearDepth;
    bool edge=visibilityConeMayIntersect(float3(0),5u,float2(0),float2(.0625),
        float3(.059,.029,.999),float3(.061,.031,1.001),2.0,nearDepth);
    cases++; if(!edge) failures++;
    float4 clear=float4(.9,-1,0,0);
    cases++; if(visibilityCertificate(clear,float3(0),float3(0,0,.8),5u)!=1)failures++;
    cases++; if(visibilityCertificate(clear,float3(0),float3(0,0,1),5u)!=-1)failures++;
    float4 opaque=float4(.1,0.5,0.5,0.5);
    cases++; if(visibilityCertificate(opaque,float3(.5,.5,0),float3(.5,.5,1),5u)!=0)failures++;
    cases++; if(visibilityCertificate(opaque,float3(.5,.5,0),float3(.51,.5,1),5u)!=-1)failures++;
    // All 12 original receiver-facing taps retain exactly the same origins and
    // normals when indexed in the 24-origin source-space cache.
    for(uint oct=0u;oct<8u;++oct)for(uint tap=0u;tap<12u;++tap)
    {
        float3 center=float3(.5),half=float3(.003,.005,.007);
        float3 receiver=float3((oct&1u)!=0u,(oct&2u)!=0u,(oct&4u)!=0u);
        LocalEmitterSample sample=sampleLocalEmitterIncident(center,half,2.0,receiver,tap);
        LocalEmitterSample source=localEmitterSource(center,half,localEmitterSourceId(center,receiver,tap));
        cases++; if(any(sample.position!=source.position)||any(sample.normal!=source.normal))failures++;
    }
    printf("visibility_certificates cases=%u failures=%u\n",cases,failures);
    return failures==0u ? 0 : 1;
}
