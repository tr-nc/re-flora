module composition_scene;

import composition_sky;
import composition_types;
import ray;
import tracer_types;

public float3 combineSceneColors(float rasterDepth, float computeDepth,
                                 float4 rasterColor, float3 terrainColor,
                                 float2 screenUv, U_GuiInput gui,
                                 U_SunInfo sun, U_CameraInfo camera,
                                 U_StarlightInfo starlight,
                                 Sampler2D sunSprite)
{
    bool hasRaster = rasterColor.a > 0.0 && rasterDepth < 1.0;
    bool hasRayTracing = computeDepth < 1.0;
    float rayTracingDepth = hasRayTracing ? computeDepth : 1.0;
    float alpha = clamp(rasterColor.a, 0.0, 1.0);

    if (hasRayTracing)
    {
        if (!hasRaster || rasterDepth > rayTracingDepth) return terrainColor;
        return rasterColor.rgb + terrainColor * (1.0 - alpha);
    }
    if (hasRaster && alpha >= 0.999) return rasterColor.rgb;

    float3 skyColor = sampleScreenSky(screenUv, gui, sun, camera, starlight, sunSprite);
    if (!hasRaster) return skyColor;
    return rasterColor.rgb + skyColor * (1.0 - alpha);
}
