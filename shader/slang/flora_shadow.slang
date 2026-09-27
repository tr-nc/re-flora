module flora_shadow;

import environment_lighting;
import sunlight;
import tracer_types;
import tracer_shadowing;

public float directionWeight(float3 shadingNormal, float3 sunDirection)
{
    float normalLengthSquared = dot(shadingNormal, shadingNormal);
    if (normalLengthSquared <= 1.0e-8) return 1.0;
    float3 normal = shadingNormal * rsqrt(normalLengthSquared);
    float negativeSide = max(0.0, dot(-normal, sunDirection));
    return max(0.7, 1.0 - negativeSide * negativeSide);
}

public struct StylizedVoxelShadowSample
{
    public float sun_transmittance;
    public float weight;
};

public StylizedVoxelShadowSample sampleStylizedVoxelShadowAtLeafReceiver(
    U_GuiInput gui, U_SunInfo sun, U_ShadowCameraInfo shadowCamera,
    float3 voxelCenter, float3 leafReceiverCenter, float3 shadingNormal)
{
    DirectSunShadowReceiver receiver = withLeafReceiver(
        pointSunShadowReceiver(voxelCenter), leafReceiverCenter);
    float transmittance = sampleDirectSunShadow(receiver, gui, shadowCamera).combined;
    StylizedVoxelShadowSample result;
    result.sun_transmittance = transmittance;
    result.weight =
        result.sun_transmittance * directionWeight(shadingNormal, sun.sun_dir);
    return result;
}

public StylizedVoxelShadowSample sampleStylizedVoxelShadow(
    U_GuiInput gui, U_SunInfo sun, U_ShadowCameraInfo shadowCamera,
    float3 voxelCenter, float3 shadingNormal)
{
    return sampleStylizedVoxelShadowAtLeafReceiver(
        gui, sun, shadowCamera, voxelCenter, voxelCenter, shadingNormal);
}

public float stylizedVoxelShadowWeight(
    U_GuiInput gui, U_SunInfo sun, U_ShadowCameraInfo shadowCamera,
    float3 voxelCenter, float3 shadingNormal)
{
    return sampleStylizedVoxelShadow(
        gui, sun, shadowCamera, voxelCenter, shadingNormal).weight;
}

public float3 applyStylizedVoxelLighting(U_GuiInput gui, U_SunInfo sun,
                                         U_ShadingInfo shading,
                                         float3 baseColor, float shadowWeight,
                                         float3 voxelCenter,
                                         float3 shadingNormal)
{
    float3 environmentLight = sampleDiffuseEnvironment(
        gui, shading, voxelCenter, shadingNormal);
    return applyStylizedVoxelLightingWithEnvironment(
        sun, baseColor, shadowWeight, environmentLight);
}

public float3 applyStylizedVoxelLightingWithEnvironment(
    U_SunInfo sun, float3 baseColor, float shadowWeight,
    float3 environmentLight)
{
    float sunLuminance = sunLuminanceFromDirection(sun.sun_dir, sun.sun_luminance);
    float3 sunLight = sun.sun_color * sunLuminance;
    return baseColor * (sunLight * shadowWeight + environmentLight);
}

// main's raster flora ambient default was #181818, uploaded as RGB / 255.
public static const float3 LEGACY_RASTER_FLORA_AMBIENT_LIGHT =
    float3(24.0 / 255.0);

public float3 applyLegacyRasterFloraLighting(
    U_SunInfo sun, float3 baseColor, float shadowWeight)
{
    float sunLuminance = sunLuminanceFromDirection(sun.sun_dir, sun.sun_luminance);
    float3 sunLight = sun.sun_color * sunLuminance;
    return baseColor *
           (sunLight * shadowWeight +
            LEGACY_RASTER_FLORA_AMBIENT_LIGHT * sun.sky_light_strength);
}
