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
