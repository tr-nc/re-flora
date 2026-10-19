module sprinkler_spray_contact;

import sprinkler_mist_math;
import sprinkler_mist_transport;
import sprinkler_spray;

// A first-contact boundary of the SAME finite spray support used in the air.
// There is no independent watering radius, sector, or guessed angle lag.
public struct B_SprinklerWaterContact {
    public float4 center_flux;
    public float4 radii;
    public float4 tangent_rotary;
    public float4 normal;
    public float4 projection; // impact direction.xyz, absolute surface cosine.w
};

public struct PushConstantSprinklerWetting {
    public uint instance;
    public uint samples;
    public uint seed;
    public float amount;
    public float patch_size;
    public uint invocations;
};

public bool sprinklerArrivesInBin(float hitAge,MistSpraySource source)
{
    return hitAge>=source.age-source.width*0.5 && hitAge<source.age+source.width*0.5;
}

public B_SprinklerWaterContact sprinklerWaterContact(MistSpraySource source,float3 position,
                                                    float3 normal,float3 target,float seconds,float hitAge)
{
    B_SprinklerWaterContact contact;
    float3 tangent=mistSprayAgeTangent(source,target,seconds);
    contact.center_flux=float4(position,source.emission*0.5);
    contact.radii=float4(mistSprayRadii(source,tangent),0);
    contact.tangent_rotary=float4(normalize(tangent),1);
    contact.normal=float4(normal,0);
    float3 velocity=target+(source.velocity-target)*exp(-hitAge/MIST_FLOW_DRAG_TIME);
    // Project ALONG arriving water, not straight down the surface normal.
    // Otherwise vertical velocity dispersion loses its change in flight time/
    // radial reach, particularly in narrow, steep deflected streams.
    // An effectively tangent/ambiguous sweep uses normal projection to keep its
    // finite contact bounded rather than dividing by an unresolved incidence.
    float3 direction=dot(velocity,normal)<-1e-5?normalize(velocity):-normal;
    contact.projection=float4(direction,abs(dot(direction,normal)));
    return contact;
}

public void sprinklerContactBounds(B_SprinklerWaterContact contact,out float3 lo,out float3 hi)
{
    float3 x,y,z;mistSprayBasis(contact.tangent_rotary,x,y,z);
    float3 d=contact.projection.xyz/dot(contact.projection.xyz,contact.normal.xyz);
    float3 extent=abs(x-d*dot(x,contact.normal.xyz))*contact.radii.x+
        abs(y-d*dot(y,contact.normal.xyz))*contact.radii.y+
        abs(z-d*dot(z,contact.normal.xyz))*contact.radii.z;
    lo=contact.center_flux.xyz-extent;hi=contact.center_flux.xyz+extent;
    for(uint axis=0;axis<3;++axis)if(abs(contact.normal[axis])>0.999) {
        // Only the first soil layer: normal moisture transfer owns absorption
        // into deeper soil. Do not soak the whole vertical column behind a hit.
        lo[axis]=contact.center_flux[axis]-max(contact.normal[axis],0.0)/256.0-1e-6;
        hi[axis]=contact.center_flux[axis]-min(contact.normal[axis],0.0)/256.0+1e-6;
    }
}

public float sprinklerContactDensity(B_SprinklerWaterContact contact,float3 point)
{
    if(contact.center_flux.w<=0.0)return 0.0;
    float depth=dot(point-contact.center_flux.xyz,contact.normal.xyz);
    if(depth>1e-6 || depth<(-1.0/256.0-1e-6))return 0.0;
    // Integrate the same finite volume along its arrival direction. Multiplying
    // by the surface cosine preserves its unit area integral at oblique hits.
    // Center the finite ray around the volume, not around a distant point on
    // the projected footprint; otherwise grazing contacts would truncate it.
    float radius=length(contact.radii.xyz)*2.0;
    // A soil cell represents its surface face, not a second hit half a voxel
    // underground. Otherwise oblique footprints would drift outside the bounds.
    float3 projection=contact.projection.xyz,offset=point-contact.center_flux.xyz-contact.normal.xyz*depth;
    float3 origin=mistSprayToLocal(offset-projection*(dot(offset,projection)+radius),contact.tangent_rotary);
    float3 direction=mistSprayToLocal(projection,contact.tangent_rotary);
    float2 interval;
    if(!mistRayInterval(origin,direction,float3(0),contact.radii.xyz,2.0*radius,interval))return 0.0;
    return mistKernelLineIntegral(origin,direction,float3(0),contact.radii.xyz,interval)*
        mistKernelAmplitude(contact.center_flux.w*contact.projection.w,contact.radii.xyz);
}
