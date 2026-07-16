module composition_cloud_reflection;

import clouds;
import composition_sky;
import composition_types;
import tracer_types;

public float3 computeSkyReflectionWithClouds(float3 viewDirection,
                                             U_GuiInput gui,
                                             U_SunInfo sun,
                                             U_CameraInfo camera,
                                             U_EnvInfo environment,
                                             U_StarlightInfo starlight,
                                             Sampler2D sunSprite)
{
    float3 skyColor = computeSkyWithSunAndStars(viewDirection, sun, camera,
                                                starlight, sunSprite);
    if (gui.clouds_enabled == 0u) return skyColor;

    uint reflectionSteps = max(3u, min(gui.cloud_primary_steps / 2u, 8u));
    CloudInputs cloudInputs = makeCloudInputs(gui, sun, camera, environment);
    CloudRaymarchResult cloudResult = computeVolumetricClouds(
        cloudInputs, viewDirection, reflectionSteps);
    return applyCloudResult(skyColor, cloudResult);
}
