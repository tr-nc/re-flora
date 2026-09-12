module leaf_lighting;

import leaf_flutter;
import sunlight;
import tracer_types;

// Shared by attached voxel leaves and detached sprites. Evaluate once per leaf
// center; optical orientation never changes geometry or shadow receiver normals.
public float3 shadeLeafWithEnvironment(float3 baseColor, float3 environmentLight,
    float sunTransmittance, float3 leafCenter, float3 opticalNormal,
    float3 cameraPosition, U_SunInfo sun, float transmissionStrength)
{
    float3 toCamera = cameraPosition - leafCenter;
    float3 view = toCamera / max(length(toCamera), 0.00001);
    float3 sunRadiance = sun.sun_color * sunLuminanceFromDirection(sun.sun_dir, sun.sun_luminance);
    return leafOpticalColor(baseColor, environmentLight, sunRadiance,
        sunTransmittance, view, normalize(sun.sun_dir), opticalNormal,
        transmissionStrength);
}
