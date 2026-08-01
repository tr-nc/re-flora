module environment_lighting;

import ddgi_query;
import tracer_types;

public float3 sampleDiffuseEnvironment(U_ShadingInfo lighting,
                                       float3 worldPosition,
                                       float3 surfaceNormal)
{
    return sampleDdgiDiffuseEnvironment(
        lighting, worldPosition, surfaceNormal).irradiance;
}

public float3 sampleEnvironmentIrradiance(U_ShadingInfo lighting,
                                           float3 worldPosition,
                                           float3 surfaceNormal)
{
    return sampleDiffuseEnvironment(lighting, worldPosition, surfaceNormal);
}
