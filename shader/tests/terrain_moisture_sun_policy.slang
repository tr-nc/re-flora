import terrain_moisture_sun;
import tracer_shadowing;

export __extern_cpp int main()
{
    float3 up = float3(0,1,0);
    // Exhaust both masks and both coverage states. Leaf readiness must never
    // stand in for terrain readiness, and fallback must keep the leaf policy.
    for (uint wanted = 0u; wanted <= DIRECT_SUN_SHADOW_SOURCE_ALL; ++wanted)
    for (uint available = 0u; available <= DIRECT_SUN_SHADOW_SOURCE_ALL; ++available)
    for (uint covered = 0u; covered < 2u; ++covered)
    {
        SoilSunExposurePlan plan = planSoilSunExposure(up, up, wanted, available, covered != 0u);
        bool terrainWanted = (wanted & DIRECT_SUN_SHADOW_SOURCE_TERRAIN) != 0u;
        bool terrainReady = (available & DIRECT_SUN_SHADOW_SOURCE_TERRAIN) != 0u && covered != 0u;
        if (!plan.lit || plan.trace_terrain != (terrainWanted && !terrainReady)) return 1;
        uint expectedMask = terrainWanted && !terrainReady ? wanted & ~DIRECT_SUN_SHADOW_SOURCE_TERRAIN : wanted;
        if (plan.direct_source_mask != expectedMask) return 2;
        SoilSunExposurePlan night = planSoilSunExposure(up, -up, wanted, available, covered != 0u);
        SoilSunExposurePlan back = planSoilSunExposure(-up, up, wanted, available, covered != 0u);
        SoilSunExposurePlan grazing = planSoilSunExposure(float3(1,0,0), up, wanted, available, covered != 0u);
        if (night.lit || night.trace_terrain || back.lit || back.trace_terrain || grazing.lit || grazing.trace_terrain) return 3;
    }
    return 0;
}
