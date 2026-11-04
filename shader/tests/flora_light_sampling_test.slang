import flora_light_sampling;

// Exercise the actual sampler in source and compacted/reversed invocation order.
// Neither dispatch row nor group membership is a sampling input.
export __extern_cpp int main()
{
    uint2 expected[128];
    for(uint source=0u;source<128u;++source)
        expected[source]=floraLightProposalRandom(
            floraLightReceiverSeed(uint3(source,19u,37u),2u,5u),7u);
    uint failures=0u;
    for(uint pass=0u;pass<8u;++pass)
    {
        for(uint row=0u;row<64u;++row)
        {
            uint source=(127u-row*2u+pass*17u)&127u;
            uint2 actual=floraLightProposalRandom(
                floraLightReceiverSeed(uint3(source,19u,37u),2u,5u),7u);
            if(any(actual!=expected[source])) ++failures;
        }
    }
    uint base=floraLightReceiverSeed(uint3(1u,19u,37u),2u,5u);
    if(base==floraLightReceiverSeed(uint3(1u,19u,38u),2u,5u)) ++failures;
    if(base==floraLightReceiverSeed(uint3(1u,19u,37u),3u,5u)) ++failures;
    if(base==floraLightReceiverSeed(uint3(1u,19u,37u),2u,6u)) ++failures;
    for(uint p=0u;p<256u;++p)
    {
        uint2 sample=floraLightProposalRandom(base,p);
        if(sample.x>=4096u || sample.y>=12u) ++failures;
    }
    printf("flora RIS stable receiver sampling failures=%u reordered_calls=512\n",failures);
    return failures==0u ? 0 : 1;
}
