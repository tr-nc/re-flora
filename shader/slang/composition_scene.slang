module composition_scene;

import composition_sky;
import ordered_dither;
import composition_types;
import ray;
import scene_depth;
import tracer_types;

public float3 combineSceneColors(float rasterDepth, float computeDepth,
                                 float4 rasterColor, float3 terrainColor,
                                 float2 screenUv,
                                 U_SunInfo sun, U_CameraInfo camera,
                                 U_StarlightInfo starlight,
                                 Sampler2D sunSprite, U_GuiInput gui)
{
    bool hasRaster = rasterColor.a > 0.0 && sceneHasDepth(rasterDepth);
    bool hasRayTracing = sceneHasDepth(computeDepth);
    float rayTracingDepth = hasRayTracing ? computeDepth : 1.0;
    float alpha = clamp(rasterColor.a, 0.0, 1.0);

    if (hasRayTracing)
    {
        if (!hasRaster || rasterDepth > rayTracingDepth) return terrainColor;
        return rasterColor.rgb + terrainColor * (1.0 - alpha);
    }
    if (hasRaster && alpha >= 0.999) return rasterColor.rgb;

    float3 skyColor = sampleScreenSky(screenUv, sun, camera, starlight, sunSprite);
    skyColor = orderedDitherColor(skyColor, orderedSceneCell(screenUv, gui.ordered_pixel_extent),
        gui.ordered_dither, gui.ordered_dither_strength, ORDERED_SKY_BACKGROUND);
    if (!hasRaster) return skyColor;
    return rasterColor.rgb + skyColor * (1.0 - alpha);
}
