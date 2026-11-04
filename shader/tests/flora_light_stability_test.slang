import flora_light_stability;

export __extern_cpp int main()
{
    uint failures=0u;
    uint4 receiver=uint4(100u,120u,140u,42u);
    uint3 revision=uint3(3u,7u,5u);
    uint4 signature=uint4(4u,5u,6u,7u);
    FloraLightStability state={};
    if(floraLightStabilityMatches(state,receiver,revision))++failures;
    state.receiver=receiver;
    state.revision_mode=uint4(revision,1u);
    state.signature=signature;
    if(!floraLightStabilityMatches(state,receiver,revision))++failures;
    if(floraLightStabilityNeedsExact(state,signature))++failures;
    // Neither compacted rows, workgroups nor camera position are state keys.
    for(uint kind=0u;kind<5u;++kind)
    {
        uint4 r=receiver;uint3 v=revision;
        if(kind==0u)r.x++;
        if(kind==1u)r.w++; // deletion/recreation at exactly the same root
        if(kind==2u)v.x++;
        if(kind==3u)v.y++; // full 64-bit light revision is retained
        if(kind==4u)v.z++;
        if(floraLightStabilityMatches(state,r,v))++failures;
    }
    for(uint channel=0u;channel<4u;++channel)
    {
        uint4 changed=signature;changed[channel]++;
        if(!floraLightStabilityNeedsExact(state,changed))++failures;
    }
    state.revision_mode.w=2u;
    if(!floraLightStabilityNeedsExact(state,signature))++failures;
    // Stable high-error estimates are corrected before they can flicker later.
    if(!floraLightStabilityCalibration(float3(1.0),float3(2.0),float3(1.0)))++failures;
    if(floraLightStabilityCalibration(float3(1.0),float3(1.01),float3(1.0)))++failures;
    if(!floraLightStabilityCalibration(float3(0.1,0.2,0.1),float3(0.2,0.1703,0.1),float3(0.1)))++failures;
    if(floraLightStabilityCalibration(float3(0.0),float3(0.0),float3(0.0)))++failures;
    printf("flora RIS adaptive stability failures=%u invalidation_cases=5 connection_changes=4\n",failures);
    return failures==0u?0:1;
}
