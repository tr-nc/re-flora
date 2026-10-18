import terrain_wetting;
import terrain_soil;

float samplePatch(float3 p,float scale,uint seed)
{
    return wetPatchUniform(p,scale,seed);
}

float expectedWet(float dose)
{
    return 3.0-exp(-dose)*(3.0+2.0*dose+0.5*dose*dose);
}

export __extern_cpp int main()
{
    uint failures=0;
    if(abs(wetPatchNormalCdf(0.0)-0.5)>0.000001 ||
       abs(wetPatchNormalCdf(-1.0)-0.158655254)>0.000001 ||
       abs(wetPatchNormalCdf(2.0)-0.977249868)>0.000001)failures++;
    if(wetPatchPoissonIncrement(0.0,1.0)!=0u || wetPatchPoissonIncrement(40.0,0.5)!=3u)failures++;
    if(wetPatchPoissonIncrement(1e-8,5e-9)!=1u ||
       wetPatchPoissonIncrement(1e-8,2.5e-17)!=2u ||
       wetPatchPoissonIncrement(1e-8,1e-25)!=3u ||
       wetPatchPoissonIncrement(1e-8,1.0)!=0u)failures++;

    // Real receivers see the same marginal dose at every patch scale, not the
    // non-uniform distribution produced by interpolating uniform random values.
    for(uint k=0;k<4u;++k)
    {
        float scale=k==0u?2.0:k==1u?4.0:k==2u?12.0:32.0;
        float sum=0,meanUniform=0;
        uint histogram[4]={0u,0u,0u,0u};
        for(uint trial=0;trial<32768u;++trial)
        {
            uint seed=trial*1664525u+1013904223u;
            float u=samplePatch(float3(196.5,127.5,360.5),scale,seed);
            uint added=wetPatchPoissonIncrement(0.4,u);
            sum+=float(added);meanUniform+=u;histogram[added]++;
        }
        float mean=sum/32768.0;
        printf("wetting scale=%.1f mean=%.5f uniform=%.5f\n",scale,mean,meanUniform/32768.0);
        if(abs(mean-expectedWet(0.4))>0.016 || abs(meanUniform/32768.0-0.5)>0.008)failures++;
        float p0=exp(-0.4);
        float4 expected=float4(p0,0.4*p0,0.08*p0,1.0-1.48*p0);
        for(uint level=0u;level<4u;++level)
            if(abs(float(histogram[level])/32768.0-expected[level])>0.012)failures++;
    }

    // Same constant dose, changing only spatial correlation. Independent voxel
    // rounding has ~0.375 disagreement; soft connected patches must reduce it.
    float previousBoundary=1.0;
    for(uint k=0;k<3u;++k)
    {
        float scale=k==0u?4.0:k==1u?12.0:24.0;
        uint boundaries=0,edges=0,wet=0;
        for(uint s=0;s<16u;++s)
        {
            uint seed=s*1664525u+1013904223u;
            for(uint z=0;z<64u;++z)for(uint x=0;x<63u;++x)
            {
                float3 p=float3(float(x)+0.5,127.5,float(z)+0.5);
                bool first=wetPatchPoissonIncrement(0.287682072,wetPatchUniform(p,scale,seed))>0u;
                bool next=wetPatchPoissonIncrement(0.287682072,wetPatchUniform(p+float3(1,0,0),scale,seed))>0u;
                boundaries+=first!=next?1u:0u;edges++;wet+=first?1u:0u;
            }
        }
        float boundary=float(boundaries)/float(edges);
        printf("wetting scale=%.1f adjacent_disagreement=%.5f wet_fraction=%.5f\n",scale,boundary,float(wet)/float(edges));
        if(boundary>=previousBoundary || boundary>0.14)failures++;
        previousBoundary=boundary;
    }

    // Coarse and fine temporal partitions have the same complete moisture
    // distribution, even when a coarse dose exceeds one wetness level.
    for(uint k=0;k<5u;++k)
    {
        uint steps=k==0u?1u:k==1u?9u:k==2u?18u:k==3u?36u:86u;
        float sum=0;
        uint histogram[4]={0u,0u,0u,0u};
        for(uint trial=0;trial<32768u;++trial)
        {
            uint moisture=0;
            for(uint step=0;step<steps;++step)
            {
                float u=terrainSoilHash01(uint3(trial,step,steps),0x68BC21EBu);
                moisture=min(3u,moisture+wetPatchPoissonIncrement(1.98/float(steps),u));
            }
            sum+=float(moisture);histogram[moisture]++;
        }
        float mean=sum/32768.0;
        printf("wetting time_steps=%u mean=%.5f dry_fraction=%.5f\n",steps,mean,float(histogram[0])/32768.0);
        if(abs(mean-expectedWet(1.98))>0.025 || abs(float(histogram[0])/32768.0-exp(-1.98))>0.009)failures++;
        float p0=exp(-1.98);
        float4 expected=float4(p0,1.98*p0,0.5*1.98*1.98*p0,
            1.0-p0*(1.0+1.98+0.5*1.98*1.98));
        for(uint level=0u;level<4u;++level)
            if(abs(float(histogram[level])/32768.0-expected[level])>0.012)failures++;
    }
    printf("terrain_wetting failures=%u\n",failures);
    return int(failures);
}
