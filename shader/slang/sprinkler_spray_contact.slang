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
                                                    float3 normal,float3 target,float seconds)
{
    B_SprinklerWaterContact contact;
    float3 tangent=mistSprayAgeTangent(source,target,seconds);
    contact.center_flux=float4(position,source.emission*0.5);
    contact.radii=float4(mistSprayRadii(source,tangent),0);
    contact.tangent_rotary=float4(normalize(tangent),1);
    contact.normal=float4(normal,0);
    return contact;
}

public void sprinklerContactBounds(B_SprinklerWaterContact contact,out float3 lo,out float3 hi)
{
    float3 extent=mistSprayBoundsRadius(contact.radii.xyz,contact.tangent_rotary);
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
    // Project the exact normalized finite volume onto its hit surface. The
    // area integral is one at every width/yaw, so widening redistributes water
    // rather than creating it. Ray parameter stays in world-distance units.
    float radius=length(contact.radii.xyz)*2.0;
    float3 origin=mistSprayToLocal(point-contact.center_flux.xyz-contact.normal.xyz*radius,contact.tangent_rotary);
    float3 direction=mistSprayToLocal(contact.normal.xyz,contact.tangent_rotary);
    float2 interval;
    if(!mistRayInterval(origin,direction,float3(0),contact.radii.xyz,2.0*radius,interval))return 0.0;
    return mistKernelLineIntegral(origin,direction,float3(0),contact.radii.xyz,interval)*
        mistKernelAmplitude(contact.center_flux.w,contact.radii.xyz);
}
