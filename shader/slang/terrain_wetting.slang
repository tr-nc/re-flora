module terrain_wetting;

import terrain_soil;

// Independent Gaussian lattice values, not interpolated uniform thresholds:
// the latter would bias small water doses toward zero. Absolute atlas positions
// keep the field continuous across compute workgroups and terrain chunks.
float wetPatchGaussian(int3 cell,uint seed)
{
    uint3 key=uint3(cell);
    float u=max(terrainSoilHash01(key,seed),0.5/16777216.0);
    float phase=6.28318530718*terrainSoilHash01(key,seed^0xB5297A4Du);
    return sqrt(-2.0*log(u))*cos(phase);
}

public float wetPatchNormalCdf(float value)
{
    float x=abs(value);
    float t=1.0/(1.0+0.2316419*x);
    float polynomial=t*(0.319381530+t*(-0.356563782+t*(1.781477937+
                     t*(-1.821255978+t*1.330274429))));
    float tail=0.3989422804*exp(-0.5*x*x)*polynomial;
    // Do not subtract from one in the lower tail: tiny wetting probabilities
    // must remain representable rather than cancelling to zero.
    return saturate(value<0.0?tail:1.0-tail);
}

// Each lattice site is a wet-patch source, with compact, overlapping support.
// Test source centers, never receiving voxels, against the two opposite 90-degree
// sectors. Axis need not be normalized; the two hardware axes remain separate.
public bool wetPatchCenterInSector(float2 offset,float2 axis)
{
    float axisSquared=dot(axis,axis);
    float along=dot(offset,axis);
    return axisSquared>1e-10 && 2.0*along*along>=dot(offset,offset)*axisSquared;
}

// Conservative broad phase: a contributing lattice site is less than one scale
// away on either XZ coordinate. Outside this band ALL source kernels are zero;
// this is not a new angular crop of a live patch.
public bool wetPatchCanReachSector(float2 offset,float2 axis,float scale)
{
    float axisLength=length(axis);
    return axisLength>1e-5 && abs(dot(offset,axis))+(2.0*max(scale,2.0)+0.001)*axisLength>=
        abs(offset.x*axis.y-offset.y*axis.x);
}

public struct WetPatchSample
{
    public float uniform;
    public float coverage;
};

WetPatchSample wetPatchField(float3 voxelPosition,float scale,uint seed,
                            float2 source,float2 axis,bool directional)
{
    // Per-dose grid jitter prevents a persistent box/grid imprint. Local
    // coefficients avoid one random amplitude changing the whole wet footprint.
    float3 shift=float3(terrainSoilHash01(uint3(13,29,71),seed),
        terrainSoilHash01(uint3(83,19,31),seed),terrainSoilHash01(uint3(43,53,7),seed));
    scale=max(scale,2.0);
    float3 p=voxelPosition/scale+shift;
    int3 cell=int3(floor(p));
    float3 f=frac(p);
    float3 blend=f*f*(3.0-2.0*f);
    float normal=0.0,variance=0.0,allowedVariance=0.0;
    [ForceUnroll] for(uint i=0u;i<8u;++i)
    {
        int3 corner=int3(i&1u,(i>>1u)&1u,(i>>2u)&1u);
        float3 w=lerp(1.0-blend,blend,float3(corner));
        float weight=w.x*w.y*w.z;
        float weightSquared=weight*weight;
        variance+=weightSquared;
        float2 patchCenter=(float3(cell+corner)-shift).xz*scale;
        if(!directional || wetPatchCenterInSector(patchCenter-source,axis))
        {
            normal+=weight*wetPatchGaussian(cell+corner,seed^0x1B56C4E9u);
            allowedVariance+=weightSquared;
        }
    }
    WetPatchSample result;
    // Normalize random thresholds by the contributors' variance, but dose by
    // ALL kernel weights. Normalizing both by only allowed sites would turn a
    // tiny kernel tail outside the sector into a fully watered patch.
    result.uniform=allowedVariance>0.0?
        wetPatchNormalCdf(-normal*rsqrt(allowedVariance)):0.5;
    result.coverage=allowedVariance/variance;
    return result;
}

public float wetPatchUniform(float3 voxelPosition,float scale,uint seed)
{
    return wetPatchField(voxelPosition,scale,seed,float2(0),float2(0),false).uniform;
}

public WetPatchSample wetPatchDirectionalSample(float3 voxelPosition,float scale,
                                               uint seed,float2 source,float2 axis)
{
    return wetPatchField(voxelPosition,scale,seed,source,axis,true);
}

// Independent time intervals compose as a Poisson process, including after
// saturating at the existing three moisture levels. Bernoulli rounding only
// approximates that property and changes wetting speed at large frame deltas.
// For a fixed local dose, scale never multiplies its amount. The directional
// kernel redistributes coverage at sector edges; its normalized weights sum to
// one across the two complementary axes, including outside the old sector.
public uint wetPatchPoissonIncrement(float dose,float uniform)
{
    if(dose<=0.0)return 0u;
    if(dose>=32.0)return 3u;
    float q1,q2,q3;
    if(dose<0.25)
    {
        // Stable Poisson survival tails. Subtracting a near-one CDF makes tiny
        // doses vanish, or turn a rounded uniform==1 into a spurious +3 jump.
        float x=dose;
        q1=x*(1.0+x*(-0.5+x*(1.0/6.0+x*(-1.0/24.0+x*(1.0/120.0-x/720.0)))));
        q2=x*x*(0.5+x*(-1.0/3.0+x*(1.0/8.0+x*(-1.0/30.0+x*(1.0/144.0-x/840.0)))));
        q3=x*x*x*(1.0/6.0+x*(-1.0/8.0+x*(1.0/20.0+x*(-1.0/72.0+x*(1.0/336.0-x/1920.0)))));
    }
    else
    {
        float p0=exp(-dose);
        q1=1.0-p0;
        q2=1.0-p0*(1.0+dose);
        q3=1.0-p0*(1.0+dose+0.5*dose*dose);
    }
    return (uniform<q1?1u:0u)+(uniform<q2?1u:0u)+(uniform<q3?1u:0u);
}
