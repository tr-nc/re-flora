module environment_lighting;

import ddgi_query;
import tracer_types;

public float3 sampleDiffuseEnvironment(U_GuiInput gui,
                                       U_ShadingInfo lighting,
                                       float3 worldPosition,
    float3 surfaceNormal)
{
    return sampleDdgiDiffuseEnvironment(
        lighting, worldPosition, surfaceNormal).irradiance;
}

public float3 sampleEnvironmentIrradiance(U_GuiInput gui,
                                           U_ShadingInfo lighting,
                                           float3 worldPosition,
                                           float3 surfaceNormal)
{
    return sampleDiffuseEnvironment(gui, lighting, worldPosition, surfaceNormal);
}

public float3 sampleMomentOnlyDiffuseEnvironment(U_GuiInput gui,
                                                  U_ShadingInfo lighting,
                                                  float3 worldPosition,
                                                  float3 surfaceNormal)
{
    return sampleDdgiMomentOnlyDiffuseEnvironment(
        lighting, worldPosition, surfaceNormal).irradiance;
}
