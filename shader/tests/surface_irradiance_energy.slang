import surface_irradiance;

bool close(float3 actual, float3 expected)
{
    return all(abs(actual - expected) < float3(1.0e-6));
}

SurfaceIrradianceIntegral cubeSurface(float3 sunDirection, uint faces, float visibility)
{
    static const float3 NORMALS[6] = {
        float3(1,0,0), float3(-1,0,0), float3(0,1,0),
        float3(0,-1,0), float3(0,0,1), float3(0,0,-1)
    };
    SurfaceIrradianceIntegral integral = beginSurfaceIrradiance();
    for (uint face = 0u; face < 6u; ++face)
    {
        if ((faces & (1u << face)) == 0u) continue;
        accumulateSurfaceIrradiance(1.0, dot(NORMALS[face], sunDirection),
            visibility, float3(0), integral);
    }
    return integral;
}

export __extern_cpp int main()
{
    // Actual thin-wood pattern: isolated cell = 6 exposed faces; end cell = 5;
    // straight one-voxel rod interior = 4. Compare identical unoccluded light.
    float3 sun = float3(1);
    float3 sideLight = float3(1,0,0);
    float3 isolated = finishSurfaceIrradiance(cubeSurface(sideLight, 63u, 1.0), sun);
    float3 tip = finishSurfaceIrradiance(cubeSurface(sideLight, 55u, 1.0), sun);
    float3 rod = finishSurfaceIrradiance(cubeSurface(sideLight, 51u, 1.0), sun);
    printf("isolated=%f (1/6) tip=%f (1/5) rod=%f (1/4)\n", isolated.x, tip.x, rod.x);
    bool thinWood = close(isolated, float3(1.0/6.0)) &&
        close(tip, float3(1.0/5.0)) && close(rod, float3(1.0/4.0));

    // A planar receiver retains Lambert's cosine rather than a fixed flora gain.
    SurfaceIrradianceIntegral plane = beginSurfaceIrradiance();
    accumulateSurfaceIrradiance(2.0, 0.4, 0.5, float3(0), plane);
    bool planar = close(finishSurfaceIrradiance(plane, sun), float3(0.2));
    SurfaceIrradianceIntegral front = beginSurfaceIrradiance();
    accumulateSurfaceIrradiance(1.0, 1.0, 1.0, float3(0), front);
    planar = planar && close(finishSurfaceIrradiance(front, sun), sun);
    SurfaceIrradianceIntegral grazing = beginSurfaceIrradiance();
    accumulateSurfaceIrradiance(1.0, 0.001, 1.0, float3(0), grazing);
    bool continuousGrazing = close(finishSurfaceIrradiance(grazing, sun), float3(0.001));

    // Shadows attenuate energy without renormalising it onto the lit subset.
    bool shadows = close(finishSurfaceIrradiance(cubeSurface(sideLight, 55u, 0.0), sun), float3(0)) &&
        close(finishSurfaceIrradiance(cubeSurface(sideLight, 55u, 0.5), sun), tip * 0.5);
    bool rotation = close(finishSurfaceIrradiance(cubeSurface(float3(0,0,1), 63u, 1.0), sun), isolated);
    float diagonalExpected = sqrt(3.0) / 6.0;
    bool diagonal = close(finishSurfaceIrradiance(cubeSurface(normalize(float3(1)), 63u, 1.0), sun),
        float3(diagonalExpected));

    // Wind-deformed unequal areas and partial visibility use the same measure
    // for sun and diffuse. Splitting a sample must not change the result.
    SurfaceIrradianceIntegral weighted = beginSurfaceIrradiance();
    accumulateSurfaceIrradiance(3.0, 0.8, 0.5, float3(0.1,0.2,0.3), weighted);
    accumulateSurfaceIrradiance(1.0, -0.6, 1.0, float3(0.5,0.4,0.3), weighted);
    float3 expected = float3(0.2,0.25,0.3) + sun * 0.3;
    SurfaceIrradianceIntegral split = beginSurfaceIrradiance();
    for (uint i = 0u; i < 3u; ++i)
        accumulateSurfaceIrradiance(1.0, 0.8, 0.5, float3(0.1,0.2,0.3), split);
    accumulateSurfaceIrradiance(1.0, -0.6, 1.0, float3(0.5,0.4,0.3), split);
    bool areas = close(finishSurfaceIrradiance(weighted, sun), expected) &&
        close(finishSurfaceIrradiance(split, sun), expected);
    SurfaceIrradianceIntegral scaled = beginSurfaceIrradiance();
    accumulateSurfaceIrradiance(3.0e-5, 0.8, 0.5, float3(0.1,0.2,0.3), scaled);
    accumulateSurfaceIrradiance(1.0e-5, -0.6, 1.0, float3(0.5,0.4,0.3), scaled);
    areas = areas && close(finishSurfaceIrradiance(scaled, sun), expected);
    bool empty = close(finishSurfaceIrradiance(beginSurfaceIrradiance(), sun), float3(0));
    bool night = close(finishSurfaceIrradiance(weighted, float3(0)), float3(0.2,0.25,0.3));
    float3 coloredSun = float3(2,1,0.5);
    bool color = close(finishSurfaceIrradiance(weighted, coloredSun),
        float3(0.2,0.25,0.3) + coloredSun * 0.3);
    printf("thin_wood=%d planar=%d grazing=%d shadows=%d rotation=%d diagonal=%d areas=%d empty=%d night=%d color=%d\n",
        thinWood, planar, continuousGrazing, shadows, rotation, diagonal, areas, empty, night, color);
    return thinWood && planar && continuousGrazing && shadows && rotation && diagonal && areas && empty && night && color ? 0 : 1;
}
