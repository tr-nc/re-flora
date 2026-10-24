module sprinkler_spray;

import sprinkler_animation;
import hash;
import sprinkler_mist_transport;

// One physical mouth emits a continuous distribution of deflected velocities.
// Eight quadrature bands resolve that distribution, with quadratic age sampling
// along each band to keep the near-mouth jet connected.
public static const uint MIST_ROTARY_BANDS=8u;
public struct MistSpraySource {
    public uint nozzle;
    public float age;
    public float width;
    public float emission;
    public float3 outlet;
    public float3 velocity;
    public float3 velocity_span;
    public float angular_velocity;
    public float spread;
};

public float2 mistSprayAgeInterval(uint index,uint count,bool rotary)
{
    uint ages=count/(rotary?MIST_ROTARY_BANDS:4u);
    uint n=index%ages;
    float2 fraction=float2(n,n+1u)/float(ages);
    return MIST_FLOW_LIFETIME*(rotary?fraction*fraction:fraction);
}

public float3 mistSprayDeflectedVelocity(float angle,float reach)
{
    // The baffle dissipates forward momentum and redirects the weaker water
    // more steeply. Near water has a shorter flight; the furthest jet retains
    // the former launch velocity. All velocities leave the SAME mouth.
    return sprinklerRotaryDirection(angle)*(0.65*reach)+float3(0,0.32*sqrt(reach),0);
}
public float3 mistSprayLaunchVelocity(float birth,float seconds,float phase,float reach=1.0)
{
    return mistSprayDeflectedVelocity(sprinklerRotaryPoseAngle(sprinklerRotaryAngleAtTick(birth,seconds,phase)),reach);
}

public float3 mistSprayTarget(float2 wind,bool rotary)
{
    // The directed spray contains falling droplets, not suspended smoke.
    return float3(wind.x,rotary?-0.50:-MIST_FLOW_SETTLING_SPEED,wind.y);
}

// Calibrate the baffle's discharge on level ground at its own nozzle height.
// This is only a source weight: actual collision/range still comes exclusively
// from shared transport against the scene, including cliffs and obstacles.
float mistSprayReferenceRangeSquared(float reach)
{
    float3 outlet=sprinklerRotaryOutlet(0);
    float3 velocity=mistSprayDeflectedVelocity(0,reach);
    float3 target=mistSprayTarget(float2(0),true),p,v;
    // Upper bound on the descending ground crossing. Newton stays on that side
    // for the concave-down trajectory; it does not converge to the nozzle time.
    float age=(outlet.y+(velocity.y-target.y)*MIST_FLOW_DRAG_TIME)/(-target.y);
    for(uint step=0;step<6;++step) {
        mistFlowAdvance(outlet,velocity,target,age,p,v);
        age-=p.y/min(v.y,-0.00001);
    }
    mistFlowAdvance(outlet,velocity,target,age,p,v);
    return dot(p.xz,p.xz);
}

public float mistSprayVisibleAmount(MistSpraySource s,bool rotary)
{
    // Coarse near-mouth water has less scattering area per liquid volume than
    // the fine downstream spray. Optical amount is independent of irrigation.
    float breakup=rotary?lerp(0.10,0.70,smoothstep(0.0,0.35,s.age)):1.0;
    return mistVisibleAmount(s.age,s.width,s.emission)*breakup;
}

// Integral of an on/off valve over a birth interval. This is the same source
// weight for air and arrival flux; zero duty never gets normalized back to one.
public float sprinklerPulsePrimitive(float seconds,float duty,float frequency=4.0)
{
    float cycles=seconds*frequency;
    return (floor(cycles)*duty+min(frac(cycles),duty))/frequency;
}
public float sprinklerPulseAmount(float center,float width,float duty,float frequency=4.0)
{
    if(duty<=0.0)return 0.0;
    if(duty>=1.0)return 1.0;
    return saturate((sprinklerPulsePrimitive(center+width*0.5,duty,frequency)-
        sprinklerPulsePrimitive(center-width*0.5,duty,frequency))/width);
}

// Shared launch law for the continuous field and its optical droplet samples.
public MistSpraySource mistRotaryKinematics(float age,float width,float reach,
    float angleNow,float phase,float rpm,float angleDegrees,float hardwareScale=1.0)
{
    MistSpraySource s;
    s.age=age;s.width=width;s.nozzle=0u;s.emission=1.0;
    // The launch pose is stationary within a hardware frame; don't add a continuous
    // rotational tangent that would smear water between discrete directions.
    s.angular_velocity=0.0;
    s.spread=tan(angleDegrees*3.14159265359/360.0);
    float angle=sprinklerLaunchAngle(angleNow,phase,rpm,age);
    s.outlet=sprinklerHardwareFrame(float3(0),hardwareScale).scale*sprinklerRotaryOutlet(angle);
    s.velocity=mistSprayDeflectedVelocity(angle,reach);
    s.velocity_span=float3(0);
    return s;
}

public MistSpraySource mistRotarySource(uint index,uint count,float angleNow,float phase,
    float rpm,float angleDegrees,float duty,float pulseTime,float frequency=4.0,float quadrature=0.5,float hardwareScale=1.0)
{
    float2 interval=mistSprayAgeInterval(index,count,true);
    uint band=index/(count/MIST_ROTARY_BANDS);
    float2 range=float2(band,band+1u)/float(MIST_ROTARY_BANDS);
    MistSpraySource s=mistRotaryKinematics((interval.x+interval.y)*0.5,interval.y-interval.x,
        // Fixed midpoints collapse whole velocity intervals onto a few terrain
        // height planes forever. Temporal stratification integrates that same
        // continuous distribution without painting or jittering soil receivers.
        lerp(range.x,range.y,quadrature),angleNow,phase,rpm,angleDegrees,hardwareScale);
    float angle=sprinklerLaunchAngle(angleNow,phase,rpm,s.age);
    s.velocity_span=mistSprayDeflectedVelocity(angle,range.y)-mistSprayDeflectedVelocity(angle,range.x);
    float weight=(mistSprayReferenceRangeSquared(range.y)-mistSprayReferenceRangeSquared(range.x))/
        (mistSprayReferenceRangeSquared(1)-mistSprayReferenceRangeSquared(0));
    s.emission=2.0*weight*sprinklerPulseAmount(pulseTime+frac(phase)/frequency-s.age,s.width,duty,frequency);
    return s;
}

// Fixed-size, stateless optical samples of the same emission distribution. Ages
// ADVECT with world time (unlike quadrature nodes); seed/launch velocity remain
// attached to each birth slot, never to a rendered frame. No second water source.
public MistSpraySource mistDropletSource(uint index,uint count,float time,float angleNow,
    float phase,float rpm,float angleDegrees,float duty,float pulseTime,float frequency,float hardwareScale=1.0)
{
    float seed=float(index)+frac(phase)*8192.0;
    float age=frac(time/MIST_FLOW_LIFETIME-(float(index)+hash11(seed+3.0)*0.7)/float(count))*MIST_FLOW_LIFETIME;
    float area=lerp(mistSprayReferenceRangeSquared(0),mistSprayReferenceRangeSquared(1),hash11(seed+11.0));
    float lo=0.0,hi=1.0;
    for(uint i=0;i<8u;++i) {
        float mid=(lo+hi)*0.5;
        if(mistSprayReferenceRangeSquared(mid)<area)lo=mid;else hi=mid;
    }
    MistSpraySource s=mistRotaryKinematics(age,0.0,(lo+hi)*0.5,angleNow,phase,rpm,angleDegrees,hardwareScale);
    // Finite transverse spread is sampled at birth, not screen-space noise.
    float3 sideways=normalize(float3(-s.velocity.z,0,s.velocity.x));
    float spread=length(s.velocity.xz)*s.spread;
    float radius=sqrt(1.0-pow(hash11(seed+29.0),1.0/3.0)),azimuth=hash11(seed+47.0)*6.28318530718;
    s.velocity+=sideways*(radius*cos(azimuth))*spread+
        float3(0,radius*sin(azimuth)*spread*0.5,0);
    s.emission=frac((pulseTime-age)*frequency+frac(phase))<duty?1.0:0.0;
    s.nozzle=index;
    return s;
}

public MistSpraySource mistSpraySource(uint index,uint count,float tick,float seconds,
                                      float phase,bool rotary)
{
    if(rotary)return mistRotarySource(index,count,sprinklerRotaryAngleAtTick(tick,seconds,0),phase,15.0,40.0,1.0,0.0);
    MistSpraySource s;
    s.angular_velocity=0.0;s.spread=0.18;s.velocity_span=float3(0);
    float2 interval=mistSprayAgeInterval(index,count,rotary);
    s.age=(interval.x+interval.y)*0.5;s.width=interval.y-interval.x;
    s.nozzle=index/(count/4u);
    float birth=tick-s.age/seconds;
    s.emission=mistPairEmission(birth,s.width/seconds,seconds,phase,s.nozzle);
    float cycle=2.0*max(floor(1.0/seconds+0.5),2.0);
    uint wrapped=uint(floor(birth)-floor(floor(birth)/cycle)*cycle);
    s.outlet=sprinklerOutlet(wrapped,seconds,phase,s.nozzle);
    s.velocity=mistFlowDirection(s.nozzle)*0.43+float3(0,0.22,0);
    return s;
}

// Derivative along the *current streak*, not just the velocity of one drop.
// Older water retains its birth direction. Rotation therefore contributes to
// spacing between ages even after outward momentum has decayed.
public float3 mistSprayAgeTangent(MistSpraySource s,float3 target,float seconds)
{
    float decay=exp(-s.age/MIST_FLOW_DRAG_TIME);
    float travel=MIST_FLOW_DRAG_TIME*(1.0-decay);
    float omega=s.angular_velocity;
    float3 swept=s.outlet+s.velocity*travel;
    return target+(s.velocity-target)*decay+omega*float3(swept.z,0,-swept.x);
}

public float3 mistSprayRadii(MistSpraySource s,float3 tangent)
{
    float travel=length(s.velocity.xz)*MIST_FLOW_DRAG_TIME*(1.0-exp(-s.age/MIST_FLOW_DRAG_TIME));
    // The nozzle aperture opens into a thin spray cone, then disperses with age.
    // Never enlarge all axes to bridge age samples: only the streak-aligned
    // reconstruction support follows their separation. Width stays physical.
    float lateral=0.18/256.0+travel*s.spread+0.004*s.age*s.age;
    float vertical=0.18/256.0+travel*s.spread*0.5+0.002*s.age*s.age;
    float longitudinal=max(lateral,length(tangent)*s.width*1.1);
    float3 radii=float3(longitudinal,lateral,vertical);
    // Transport the launch-velocity interval too, rather than drawing several
    // isolated center rays or filling only the soil. Its support starts at zero
    // at the mouth and opens continuously with flight time. Both consumers use
    // this same normalized finite volume.
    float response=MIST_FLOW_DRAG_TIME*(1.0-exp(-s.age/MIST_FLOW_DRAG_TIME));
    // Uniform interval variance is span²/12; the finite (1-r²)² volume has
    // coordinate variance radius²/9. Match those moments, not a fitted wet radius.
    float3 spread=mistSprayToLocal(s.velocity_span*response*sqrt(9.0/12.0),float4(tangent,1));
    return sqrt(radii*radii+spread*spread);
}

// Rigid orthonormal kernel frame. The original alternating spray retains its
// axis-aligned support; both variants use the same exact ray integral.
public void mistSprayBasis(float4 tangentRotary,out float3 x,out float3 y,out float3 z)
{
    if(tangentRotary.w==0.0) { x=float3(1,0,0);y=float3(0,1,0);z=float3(0,0,1);return; }
    x=normalize(tangentRotary.xyz);
    // Near-field drops are steep. Switching to a world-Z reference at 0.95
    // made their anisotropic support change shape as the head rotated. Keep
    // the yaw-covariant up frame unless the tangent is actually degenerate.
    y=normalize(cross(dot(x.xz,x.xz)>1e-8?float3(0,1,0):float3(0,0,1),x));
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
