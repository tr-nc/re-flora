import terrain_wetting;

float expectedWet(float dose)
{
    return 3.0-exp(-dose)*(3.0+2.0*dose+0.5*dose*dose);
}

export __extern_cpp int main()
{
    uint failures=0;
    float2 origin=float2(196.5,360.5);
    if(!wetPatchCenterInSector(float2(0,20),float2(0,1)) ||
       !wetPatchCenterInSector(float2(0,-20),float2(0,1)) ||
       !wetPatchCenterInSector(float2(20,20),float2(0,1)) ||
       wetPatchCenterInSector(float2(20,19),float2(0,1)) ||
       wetPatchCenterInSector(float2(0,20),float2(1,0)) ||
       wetPatchCenterInSector(float2(0),float2(0))) failures++;

    // Receiving points outside either diagonal boundary may still be wetted by
    // valid centers. Both opposite sectors and their four edges are symmetric.
    for(uint side=0u;side<4u;++side)
    {
        float2 offset=float2(side<2u?30.5:-30.5,(side&1u)!=0u?-29.25:29.25);
        float3 p=float3(origin.x+offset.x,127.5,origin.y+offset.y);
        float sum=0;
        for(uint s=0u;s<4096u;++s)
        {
            uint seed=s*1664525u+1013904223u;
            WetPatchSample patch=wetPatchDirectionalSample(p,12.0,seed,origin,float2(0,1));
            sum+=float(wetPatchPoissonIncrement(0.4*patch.coverage,patch.uniform));
        }
        printf("wet_sector edge=%u outside_mean=%.5f\n",side,sum/4096.0);
        if(sum<100.0)failures++;
    }

    // Fully interior behavior is unchanged, far-away excluded centers contribute
    // no water, and the conservative dispatch test never crops an active tail.
    for(uint s=0u;s<64u;++s)
    {
        uint seed=s*1664525u+1013904223u;
        for(uint a=0u;a<3u;++a)
        {
            float2 axis=a==0u?float2(0,1):a==1u?float2(1,0):normalize(float2(1,2));
            float2 offset=axis*50.0;
            float3 p=float3(origin.x+offset.x,127.5,origin.y+offset.y);
            WetPatchSample inside=wetPatchDirectionalSample(p,12.0,seed,origin,axis);
            if(inside.coverage!=1.0 || abs(inside.uniform-wetPatchUniform(p,12.0,seed))>1e-6)failures++;
            for(uint i=0u;i<65u;++i)
            {
                float angle=float(i)*6.28318530718/64.0;
                offset=60.0*float2(cos(angle),sin(angle));
                p=float3(origin.x+offset.x,127.5,origin.y+offset.y);
                WetPatchSample patch=wetPatchDirectionalSample(p,12.0,seed,origin,axis);
                if(patch.coverage<0.0 || patch.coverage>1.0)failures++;
                if(!wetPatchCanReachSector(offset,axis,12.0) && patch.coverage!=0.0)failures++;
            }
        }
        float3 far=float3(origin.x+60.0,127.5,origin.y);
        if(wetPatchDirectionalSample(far,12.0,seed,origin,float2(0,1)).coverage!=0.0)failures++;
    }

    // Complementary source-center sectors partition the input kernel, not the
    // receiving image. Their weights sum to one even where a patch crosses an
    // edge. Independent Gaussian contributors preserve the full capped-Poisson
    // dose distribution; larger patches must not multiply expected input.
    for(uint k=0u;k<4u;++k)
    {
        float scale=k==0u?2.0:k==1u?4.0:k==2u?12.0:32.0;
        float sum=0;
        uint histogram[4]={0u,0u,0u,0u};
        float3 p=float3(origin.x+30.5,127.5,origin.y+29.25);
        for(uint s=0u;s<32768u;++s)
        {
            uint seed=s*1664525u+1013904223u;
            WetPatchSample z=wetPatchDirectionalSample(p,scale,seed,origin,float2(0,1));
            WetPatchSample x=wetPatchDirectionalSample(p,scale,seed,origin,float2(1,0));
            if(abs(z.coverage+x.coverage-1.0)>0.000002)failures++;
            uint wet=min(3u,wetPatchPoissonIncrement(0.8*z.coverage,z.uniform)+
                            wetPatchPoissonIncrement(0.8*x.coverage,x.uniform));
            sum+=float(wet);histogram[wet]++;
        }
        float mean=sum/32768.0;
        printf("wet_sector scale=%.1f paired_mean=%.5f\n",scale,mean);
        if(abs(mean-expectedWet(0.8))>0.02)failures++;
        float p0=exp(-0.8);
        float4 expected=float4(p0,0.8*p0,0.32*p0,1.0-2.12*p0);
        for(uint level=0u;level<4u;++level)
            if(abs(float(histogram[level])/32768.0-expected[level])>0.012)failures++;
    }
    printf("terrain_wetting_sector failures=%u\n",failures);
    return int(failures);
}
