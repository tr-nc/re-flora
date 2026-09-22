module surface_irradiance;

// Quadrature over an exposed surface. Geometry/sampling and visibility belong
// to the caller; this reducer owns the measure used to average irradiance.
public struct SurfaceIrradianceIntegral
{
    float3 diffuse;
    float visibleSun;
    float area;
};

public SurfaceIrradianceIntegral beginSurfaceIrradiance()
{
    SurfaceIrradianceIntegral result;
    result.diffuse = float3(0);
    result.visibleSun = 0.0;
    result.area = 0.0;
    return result;
}

public void accumulateSurfaceIrradiance(float area, float sunCosine,
    float sunVisibility, float3 diffuseIrradiance,
    inout SurfaceIrradianceIntegral integral)
{
    integral.diffuse += area * diffuseIrradiance;
    float projectedArea = area * max(0.0, sunCosine);
    integral.visibleSun += projectedArea * sunVisibility;
    integral.area += area;
}

public float3 finishSurfaceIrradiance(SurfaceIrradianceIntegral integral,
    float3 sunIrradiance)
{
    // Average incoming energy over the SAME exposed area for every light source.
    // Dividing sunlight by projected (light-facing) area instead would cancel
    // Lambert's cosine and spread near-full sun over every face of a thin tip.
    // Back-facing/shadowed samples remain in the denominator, not in sun energy.
    if (integral.area <= 0.0) return float3(0);
    return (integral.diffuse + sunIrradiance * integral.visibleSun) / integral.area;
}
