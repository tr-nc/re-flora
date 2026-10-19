module sprinkler_spray;

import sprinkler_animation;
import sprinkler_mist_transport;

// A time interval of one continuously emitting outlet, not a discrete puff.
// Rotary uses the entire fixed work budget for its single outlet. Quadratic
// age spacing resolves the narrow jet at the mouth without starving the tail.
public struct MistSpraySource {
    public uint nozzle;
    public float age;
    public float width;
    public float emission;
    public float3 outlet;
    public float3 velocity;
};

public float2 mistSprayAgeInterval(uint index,uint count,bool rotary)
{
    uint ages=rotary?count:count/4u;
    uint n=index%ages;
    float2 fraction=float2(n,n+1u)/float(ages);
    return MIST_FLOW_LIFETIME*(rotary?fraction*fraction:fraction);
}

public float3 mistSprayLaunchVelocity(float birth,float seconds,float phase)
{
    return sprinklerRotaryDirectionAtTick(birth,seconds,phase)*0.65+float3(0,0.32,0);
}

public float3 mistSprayTarget(float2 wind,bool rotary)
{
    // The directed spray contains falling droplets, not suspended smoke.
    return float3(wind.x,rotary?-0.50:-MIST_FLOW_SETTLING_SPEED,wind.y);
}

public float mistSprayVisibleAmount(MistSpraySource s,bool rotary)
{
    // Coarse near-mouth water has less scattering area per liquid volume than
    // the fine downstream spray. Optical amount is independent of irrigation.
    float breakup=rotary?lerp(0.10,0.70,smoothstep(0.0,0.35,s.age)):1.0;
    return mistVisibleAmount(s.age,s.width,s.emission)*breakup;
}

public MistSpraySource mistSpraySource(uint index,uint count,float tick,float seconds,
                                      float phase,bool rotary)
{
    MistSpraySource s;
    float2 interval=mistSprayAgeInterval(index,count,rotary);
    s.age=(interval.x+interval.y)*0.5;s.width=interval.y-interval.x;
    s.nozzle=rotary?0u:index/(count/4u);
    float birth=tick-s.age/seconds;
    s.emission=rotary?mistRotaryEmission(s.nozzle):mistPairEmission(birth,s.width/seconds,seconds,phase,s.nozzle);
    if(rotary) {
        s.outlet=sprinklerRotaryOutletAtTick(birth,seconds,phase);
        s.velocity=mistSprayLaunchVelocity(birth,seconds,phase);
    } else {
        float cycle=2.0*max(floor(1.0/seconds+0.5),2.0);
        uint wrapped=uint(floor(birth)-floor(floor(birth)/cycle)*cycle);
        s.outlet=sprinklerOutlet(wrapped,seconds,phase,s.nozzle);
        s.velocity=mistFlowDirection(s.nozzle)*0.43+float3(0,0.22,0);
    }
    return s;
}

// Derivative along the *current streak*, not just the velocity of one drop.
// Older water retains its birth direction. Rotation therefore contributes to
// spacing between ages even after outward momentum has decayed.
public float3 mistSprayAgeTangent(MistSpraySource s,float3 target,float seconds)
{
    float decay=exp(-s.age/MIST_FLOW_DRAG_TIME);
    float travel=MIST_FLOW_DRAG_TIME*(1.0-decay);
    float omega=6.28318530718/(float(sprinklerRotaryTicks(seconds))*seconds);
    float3 swept=s.outlet+s.velocity*travel;
    return target+(s.velocity-target)*decay+omega*float3(swept.z,0,-swept.x);
}

public float3 mistSprayRadii(MistSpraySource s,float3 tangent)
{
    float travel=length(s.velocity.xz)*MIST_FLOW_DRAG_TIME*(1.0-exp(-s.age/MIST_FLOW_DRAG_TIME));
    // The nozzle aperture opens into a thin spray cone, then disperses with age.
    // Never enlarge all axes to bridge age samples: only the streak-aligned
    // reconstruction support follows their separation. Width stays physical.
    float lateral=0.18/256.0+travel*0.18+0.004*s.age*s.age;
    float vertical=0.18/256.0+travel*0.09+0.002*s.age*s.age;
    float longitudinal=max(lateral,length(tangent)*s.width*1.1);
    return float3(longitudinal,lateral,vertical);
}

// Rigid orthonormal kernel frame. The original alternating spray retains its
// axis-aligned support; both variants use the same exact ray integral.
public void mistSprayBasis(float4 tangentRotary,out float3 x,out float3 y,out float3 z)
{
    if(tangentRotary.w==0.0) { x=float3(1,0,0);y=float3(0,1,0);z=float3(0,0,1);return; }
    x=normalize(tangentRotary.xyz);
    y=normalize(cross(abs(x.y)<0.95?float3(0,1,0):float3(0,0,1),x));
    z=cross(x,y);
}

public float3 mistSprayToLocal(float3 value,float4 tangentRotary)
{
    float3 x,y,z;mistSprayBasis(tangentRotary,x,y,z);
    return float3(dot(value,x),dot(value,y),dot(value,z));
}

public float3 mistSprayBoundsRadius(float3 radii,float4 tangentRotary)
{
    float3 x,y,z;mistSprayBasis(tangentRotary,x,y,z);
    // Conservative AABB, used for both tile coverage and terrain proximity.
    return abs(x)*radii.x+abs(y)*radii.y+abs(z)*radii.z;
}
