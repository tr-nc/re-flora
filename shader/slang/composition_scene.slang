module composition_scene;

import composition_sky;
import composition_types;
import ray;
import tracer_types;

float3 applyScreenClouds(float3 skyColor, float2 screenUv,
                         U_GuiInput gui, Sampler2D cloudOutput)
{
    if (gui.clouds_enabled == 0u) return skyColor;
    float4 clouds = cloudOutput.SampleLevel(screenUv, 0.0);
    return lerp(skyColor, clouds.rgb, clamp(clouds.a, 0.0, 1.0));
}

public float3 combineSceneColors(float rasterDepth, float computeDepth,
                                 float4 rasterColor, float3 denoisedColor,
                                 float2 screenUv, U_GuiInput gui,
                                 U_SunInfo sun, U_CameraInfo camera,
                                 U_StarlightInfo starlight,
                                 Sampler2D sunSprite, Sampler2D cloudOutput)
{
    bool hasRaster = rasterColor.a > 0.0 && rasterDepth < 1.0;
    bool hasRayTracing = computeDepth < 1.0;
    float rayTracingDepth = hasRayTracing ? computeDepth : 1.0;
    float alpha = clamp(rasterColor.a, 0.0, 1.0);

    if (hasRayTracing)
    {
        if (!hasRaster || rasterDepth > rayTracingDepth) return denoisedColor;
        return rasterColor.rgb + denoisedColor * (1.0 - alpha);
    }
    if (hasRaster && alpha >= 0.999) return rasterColor.rgb;

    float3 viewDirection = normalize(generateCameraRay(screenUv,
                                                       camera.view_proj_mat_inv).direction);
    float3 skyColor = applyScreenClouds(
        computeSkyWithSunAndStars(viewDirection, sun, camera, starlight, sunSprite),
        screenUv, gui, cloudOutput);
    if (!hasRaster) return skyColor;
    return rasterColor.rgb + skyColor * (1.0 - alpha);
}
