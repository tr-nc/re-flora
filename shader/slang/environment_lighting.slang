module environment_lighting;

import ddgi_query;
import tracer_types;

public float3 sampleDiffuseEnvironment(U_GuiInput gui,
                                       U_ShadingInfo lighting,
                                       float3 worldPosition,
    float3 surfaceNormal)
{
    if (gui.legacy_environment_lighting != 0u)
        return gui.legacy_ambient_light;
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
