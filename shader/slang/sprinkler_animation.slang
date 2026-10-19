module sprinkler_animation;

// CPU water outlets use crate::sprinkler::Pose. Explicit half-up rounding is
// intentional: Slang round() tie behavior must not select a different pair.
public void sprinklerAnimation(uint tick, float tickSeconds, float animationPhase,
                               out float extension, out bool animateXArms)
{
    uint pairCycleTicks = max(
        uint(floor(1.0 / max(tickSeconds, 1.0 / 240.0) + 0.5)), 2u);
    uint fullCycleTicks = pairCycleTicks * 2u;
    uint phaseOffsetTicks = uint(floor(
        frac(animationPhase) * float(fullCycleTicks) + 0.5));
    uint cycleTick = (tick + phaseOffsetTicks) % fullCycleTicks;
    animateXArms = cycleTick >= pairCycleTicks;
    float pairPhase = float(cycleTick % pairCycleTicks) / float(pairCycleTicks);
    extension = 0.5 - 0.5 * cos(pairPhase * 6.28318530718);
}

// A separate original mode is retained; these functions own the new rotor pose.
public uint sprinklerRotaryTicks(float seconds)
{
    return max(uint(floor(4.0/max(seconds,1.0/240.0)+0.5)),4u);
}
public float3 sprinklerRotate(float3 value,float angle)
{
    float s=sin(angle),c=cos(angle);
    return float3(c*value.x-s*value.z,value.y,s*value.x+c*value.z);
}
public float sprinklerRotaryAngleAtTick(float tick,float seconds,float phase)
{
    return 6.28318530718*frac(tick/float(sprinklerRotaryTicks(seconds))+frac(phase));
}
public float sprinklerRotaryAngle(uint tick,float fraction,float seconds,float phase)
{
    return sprinklerRotaryAngleAtTick(float(tick%sprinklerRotaryTicks(seconds))+fraction,seconds,phase);
}
public float3 sprinklerRotaryOutletAtTick(float tick,float seconds,float phase)
{
    float angle=sprinklerRotaryAngleAtTick(tick,seconds,phase);
    return sprinklerRotate(float3(3.15,4.2,-0.85)/256.0,angle);
}
public float3 sprinklerRotaryDirectionAtTick(float tick,float seconds,float phase)
{
    float angle=sprinklerRotaryAngleAtTick(tick,seconds,phase)-0.65;
    return float3(cos(angle),0,sin(angle));
}

// Shared outlet contract for hardware pose and the continuous spray source.
public float3 sprinklerOutlet(uint tick,float tickSeconds,float phase,uint nozzle)
{
    float extension;bool alongX;
    sprinklerAnimation(tick,tickSeconds,phase,extension,alongX);
    bool xArm=nozzle<2u;
    float sign=(nozzle&1u)==0u ? 1.0 : -1.0;
    float reach=(3.1+((xArm==alongX) ? 0.2*extension : 0.0))/256.0;
    return float3(xArm ? sign*reach : 0.0,(4.0+extension)/256.0,
                  xArm ? 0.0 : sign*reach);
}

public float3 sprinklerPartTranslation(float3 motion,float extension,bool alongX)
{
    bool xArm=abs(motion.x)>0.0,zArm=abs(motion.z)>0.0;
    if((xArm||zArm)&&!(alongX?xArm:zArm))motion.xz=float2(0);
    return motion*extension;
}
