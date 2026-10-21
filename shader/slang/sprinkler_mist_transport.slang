module sprinkler_mist_transport;

// Runtime counts/tile size come from the frame owner, not a second emitter registry.
public struct PushConstantSprinklerMist {
    public uint count;
    public float density;
    public uint shadows_enabled;
    public uint glass_enabled;
    public uint flow_enabled;
    public uint lobes_per_instance;
    public uint tile_size;
    public uint width;
    public uint height;
    public uint droplets_per_instance;
    public uint particles_enabled;
    public uint air_enabled; // consumer only; preparation/contacts keep their original inputs
};

public struct B_SprinklerMistLobe {
    public float4 center_amplitude;
    public float4 radii_near_geometry;
    public float4 ambient_sun_visibility;
    public float4 tangent_rotary;
};

public struct B_SprinklerMistBounds {
    public float4 lower;
    public float4 upper;
    public float4 screen_rect;
};

public static const float MIST_FLOW_LIFETIME = 3.0;
public static const float MIST_FLOW_DRAG_TIME = 0.65;
public static const float MIST_FLOW_SETTLING_SPEED = 0.14;
// Visual liquid content / second / nozzle, unrelated to gameplay irrigation.
public static const float MIST_FLOW_SOURCE_AMOUNT = 0.00062;

public float3 mistFlowDirection(uint nozzle)
{
    return nozzle < 2u ? float3(nozzle == 0u ? 1.0 : -1.0,0,0)
                       : float3(0,0,nozzle == 2u ? 1.0 : -1.0);
}

public void mistFlowAdvance(float3 position,float3 velocity,float3 target,float dt,
                           out float3 nextPosition,out float3 nextVelocity)
{
    float decay=exp(-dt/MIST_FLOW_DRAG_TIME);
    nextPosition=position+target*dt+(velocity-target)*(MIST_FLOW_DRAG_TIME*(1.0-decay));
    nextVelocity=target+(velocity-target)*decay;
}

public float mistRotaryEmission(uint nozzle)
{
    // One mouth carries the same total visual source as the original pair.
    return nozzle==0u?2.0:0.0;
}

public float3 mistFlowRadii(float age,float ageWidth,uint nozzle)
{
    float radialSpeed=0.43*exp(-age/MIST_FLOW_DRAG_TIME);
    float longitudinal=0.027+radialSpeed*ageWidth*0.6;
    float lateral=sqrt(0.025*0.025+0.0010*age+0.0011*age*age);
    float vertical=sqrt(0.023*0.023+0.00035*age+0.00012*age*age);
    return nozzle<2u ? float3(longitudinal,vertical,lateral)
                     : float3(lateral,vertical,longitudinal);
}

public float mistVisibleAmount(float age,float ageWidth,float emission)
{
    return MIST_FLOW_SOURCE_AMOUNT*ageWidth*emission*exp(-age/1.8);
}

public float mistKernelAmplitude(float amount,float3 radii)
{
    // Integral of (1-r²)^2 over the unit ball is 32*pi/105.
    return amount*(105.0/100.5309649)/(radii.x*radii.y*radii.z);
}

float mistPairPrimitive(float tick,float pairTicks)
{
    float cycle=floor(tick/(2.0*pairTicks));
    return cycle*pairTicks+max(0.0,tick-cycle*(2.0*pairTicks)-pairTicks);
}

public float mistPairEmission(float birthTick,float widthTicks,float tickSeconds,
                              float phase,uint nozzle)
{
    float pairTicks=max(floor(1.0/max(tickSeconds,1.0/240.0)+0.5),2.0);
    float offset=floor(frac(phase)*(2.0*pairTicks)+0.5);
    float lo=birthTick+offset-widthTicks*0.5;
    float hi=lo+widthTicks;
    float xWeight=saturate((mistPairPrimitive(hi,pairTicks)-mistPairPrimitive(lo,pairTicks))/widthTicks);
    return nozzle<2u ? xWeight : 1.0-xWeight;
}

// Exact line integral of the continuous compact ellipsoid kernel. No visible
// particles and no ray-step aliasing when the spray becomes a thin curtain.
public float mistKernelLineIntegral(float3 origin,float3 direction,float3 center,
                                   float3 radii,float2 interval)
{
    float3 o=(origin-center)/radii,d=direction/radii;
    float a=dot(d,d),closest=-dot(o,d)/a;
    float3 perpendicular=o+d*closest;
    float h=1.0-dot(perpendicular,perpendicular);
    if(h<=0.0 || interval.y<=interval.x)return 0.0;
    float halfWidth=sqrt(h/a);
    float2 s=clamp((interval-closest)/halfWidth,-1.0,1.0);
    float2 s2=s*s;
    float2 primitive=s*(1.0+s2*(-2.0/3.0+s2*0.2));
    return max(0.0,h*h*halfWidth*(primitive.y-primitive.x));
}

public bool mistClipAirPlane(float3 origin,float3 direction,float3 point,float3 normal,
                             inout float2 interval)
{
    float offset=dot(origin-point,normal),slope=dot(direction,normal);
    if(abs(slope)<1.0e-9)return offset>=0.0;
    float crossing=-offset/slope;
    if(slope>0.0)interval.x=max(interval.x,crossing);
    else interval.y=min(interval.y,crossing);
    return interval.y>interval.x;
}

// Pure, bounded voxel walk shared by terrain motion, finite-kernel clipping and
// CPU shader tests. Querying occupancy/readiness belongs to the geometry owner.
public struct MistVoxelWalk {
    public int3 cell;
    public int3 step;
    public float3 next;
    public float3 delta;
    public float entry;
    public float3 normal;
};

public MistVoxelWalk mistBeginWalk(float3 start,float3 end)
{
    MistVoxelWalk w;
    float3 d=end-start;
    w.cell=int3(floor(start));w.step=int3(sign(d));w.entry=0;w.normal=float3(0);
    for(uint axis=0;axis<3;++axis) {
        w.delta[axis]=abs(d[axis])<1.0e-12 ? 1.0e30 : abs(1.0/d[axis]);
        float boundary=float(w.cell[axis]+(w.step[axis]>0 ? 1 : 0));
        w.next[axis]=w.step[axis]==0 ? 1.0e30 : max(0.0,(boundary-start[axis])/d[axis]);
    }
    return w;
}

public bool mistWalkNext(inout MistVoxelWalk w)
{
    float crossing=min(w.next.x,min(w.next.y,w.next.z));
    if(crossing>1.0 || crossing>=1.0e29)return false;
    bool3 tied=w.next<=float3(crossing+1.0e-6);
    w.normal=float3(0);
    for(uint axis=0;axis<3;++axis)if(tied[axis]) {
        w.cell[axis]+=w.step[axis];w.next[axis]+=w.delta[axis];
        w.normal[axis]=-float(w.step[axis]);
    }
    w.normal=normalize(w.normal);w.entry=crossing;
    return true;
}
