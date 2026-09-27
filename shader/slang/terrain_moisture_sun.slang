module terrain_moisture_sun;

import tracer_shadowing;

// Soil requires geometric exposure even before the visual VSM is ready or
// outside its coverage. Keep this gameplay policy independent of source maps.
public struct SoilSunExposurePlan
{
    public bool lit;
    public bool trace_terrain;
    public uint direct_source_mask;
};

public SoilSunExposurePlan planSoilSunExposure(float3 normal, float3 sunDirection,
    uint sourceMask, uint availableMask, bool insideShadowCamera)
{
    SoilSunExposurePlan plan;
    plan.lit = sunDirection.y > 0.0 && dot(normal, sunDirection) > 0.05;
    bool wantsTerrain = (sourceMask & DIRECT_SUN_SHADOW_SOURCE_TERRAIN) != 0u;
    bool readyTerrain = (availableMask & DIRECT_SUN_SHADOW_SOURCE_TERRAIN) != 0u;
    plan.trace_terrain = plan.lit && wantsTerrain && !(readyTerrain && insideShadowCamera);
    plan.direct_source_mask = plan.trace_terrain
        ? sourceMask & ~DIRECT_SUN_SHADOW_SOURCE_TERRAIN : sourceMask;
    return plan;
}
