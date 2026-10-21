module sprinkler_air_math;

import sprinkler_mist_transport;
import sprinkler_spray;

// Optical-only, current-frame field. No emitter, contact, clock or saved-world state.
public struct B_SprinklerAirInfo {
    public float4 lower_valid;
    public float4 cell_side; // xyz: isotropic cell size; w: side in cells
    public float4 ambient_mass; // one mass-weighted environment approximation
};

public struct PushConstantSprinklerAir {
    public uint count;
    public uint lobes_per_instance;
    public uint grid_side;
    public uint background_enabled;
    public uint glass_enabled;
    public uint shadows_enabled;
};

// At most 256*256 lobes deposit once per cell. 65536*65535 < UINT_MAX.
// This explicit per-contribution saturation bounds atomics without float atomics.
public static const float SPRINKLER_AIR_FIXED_SCALE = 64.0;
public uint airEncodeDensity(float density)
{
    return uint(clamp(density*SPRINKLER_AIR_FIXED_SCALE+0.5,0.0,65535.0));
}

public uint airCellIndex(uint3 cell,uint side)
{
    return (cell.z*side+cell.y)*side+cell.x;
}

public B_SprinklerAirInfo airGridInfo(float3 lo,float3 hi,uint side,float4 ambientMass)
{
    B_SprinklerAirInfo info = (B_SprinklerAirInfo)0;
    if(ambientMass.w<=0.0 || side<=4u)return info;
    float3 span=hi-lo;
    float cell=max(max(span.x,max(span.y,span.z)),1.0/256.0)/float(side-4u);
    info.lower_valid=float4((lo+hi)*0.5-float3(cell*float(side)*0.5),1);
    info.cell_side=float4(float3(cell),float(side));
    info.ambient_mass=float4(ambientMass.xyz/ambientMass.w,ambientMass.w);
    return info;
}

// Broaden sub-cell kernels, preserving their analytic mass before terrain
// rejection and quantization. This is an intentional optical approximation.
public B_SprinklerMistLobe airResampleLobe(B_SprinklerMistLobe lobe,float cell)
{
    float3 r=lobe.radii_near_geometry.xyz;
    float3 broad=sqrt(r*r+float3(cell*cell*0.75));
    lobe.center_amplitude.w *= (r.x*r.y*r.z)/(broad.x*broad.y*broad.z);
    lobe.radii_near_geometry.xyz=broad;
    return lobe;
}

public bool airRayInterval(float3 origin,float3 direction,float distance,
                          B_SprinklerAirInfo info,out float2 interval)
{
    interval=float2(0,max(0.0,distance));
    if(info.lower_valid.w==0.0)return false;
    float3 lo=info.lower_valid.xyz;
    float3 hi=lo+info.cell_side.xyz*info.cell_side.w;
    for(uint axis=0;axis<3u;++axis) {
        if(abs(direction[axis])<1e-8) {
            if(origin[axis]<lo[axis] || origin[axis]>hi[axis])return false;
        } else {
            float a=(lo[axis]-origin[axis])/direction[axis];
            float b=(hi[axis]-origin[axis])/direction[axis];
            interval.x=max(interval.x,min(a,b));
            interval.y=min(interval.y,max(a,b));
        }
    }
    return interval.y>interval.x;
}
