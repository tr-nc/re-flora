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

public float3 sprinklerPartTranslation(float3 motion,float extension,bool alongX)
{
    bool xArm=abs(motion.x)>0.0,zArm=abs(motion.z)>0.0;
    if((xArm||zArm)&&!(alongX?xArm:zArm))motion.xz=float2(0);
    return motion*extension;
}
