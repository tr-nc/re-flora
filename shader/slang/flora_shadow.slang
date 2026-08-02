module flora_shadow;

import environment_lighting;
import sunlight;
import tracer_types;
import vsm;

float4 sampleAtBaseLevel(Sampler2D textureSampler, float2 uv)
{
    return textureSampler.SampleLevel(uv, 0.0);
}

float terrainShadowTransmittance(Sampler2D vsmMap,
                                 float4x4 shadowViewProjection,
                                 float4 worldPosition)
{
    float4 lightSpace = mul(shadowViewProjection, worldPosition);
    float3 ndc = lightSpace.xyz / lightSpace.w;
    float2 uv = ndc.xy * 0.5 + 0.5;
    float2 warpedDepth = warpDepth(ndc.z, evsmExponents());
    float4 occluder = sampleAtBaseLevel(vsmMap, uv);
    float positive = chebyshevUpperBound(occluder.xz, warpedDepth.x);
    float negative = chebyshevUpperBound(occluder.yw, warpedDepth.y);
    return clamp(min(positive, negative), 0.0, 1.0);
}

float leafOpacity(Sampler2D opacityMap, U_GuiInput gui, float2 uv,
                  float receiverDepth)
{
    uint width;
    uint height;
    uint levels;
    opacityMap.GetDimensions(0, width, height, levels);
    float2 radius = (1.0 / float2(width, height)) *
                    max(gui.leaf_shadow_filter_radius_texels, 0.0);
    float opacity = 0.0;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            float4 sampleValue = sampleAtBaseLevel(
                opacityMap, uv + float2(float(x), float(y)) * radius);
            float sampleOpacity = clamp(sampleValue.a, 0.0, 1.0);
            if (sampleOpacity > 1.0e-4)
            {
                float casterDepth = clamp(sampleValue.r / sampleOpacity, 0.0, 1.0);
                if (receiverDepth <= casterDepth + 0.0015) sampleOpacity = 0.0;
            }
            opacity = max(opacity, sampleOpacity);
        }
    }
    return clamp(opacity, 0.0, 1.0);
}

float leafShadowTransmittance(Sampler2D opacityMap, Sampler2D maskMap,
                              U_GuiInput gui, float4x4 shadowViewProjection,
                              float4 worldPosition)
{
    float4 lightSpace = mul(shadowViewProjection, worldPosition);
    float3 ndc = lightSpace.xyz / lightSpace.w;
    float2 uv = ndc.xy * 0.5 + 0.5;
    if (any(uv < float2(0.0)) || any(uv > float2(1.0))) return 1.0;

    float mask = sampleAtBaseLevel(maskMap, uv).a;
    if (mask < 0.01) return 1.0;
    float opacity = leafOpacity(opacityMap, gui, uv, ndc.z) * mask;
    float transmittance = 1.0 - opacity * max(gui.leaf_shadow_strength, 0.0);
    return clamp(transmittance,
                 clamp(gui.leaf_shadow_min_transmittance, 0.0, 1.0), 1.0);
}

float cloudShadowTransmittance(Sampler2D cloudShadowMap, U_GuiInput gui,
                               float4x4 shadowViewProjection,
                               float4 worldPosition)
{
    if (gui.cloud_shadows_enabled == 0u || gui.clouds_enabled == 0u) return 1.0;
    if (worldPosition.y > gui.cloud_top_height) return 1.0;

    float4 lightSpace = mul(shadowViewProjection, worldPosition);
    float3 ndc = lightSpace.xyz / lightSpace.w;
    float2 uv = ndc.xy * 0.5 + 0.5;
    if (any(uv < float2(0.0)) || any(uv > float2(1.0))) return 1.0;
    return clamp(sampleAtBaseLevel(cloudShadowMap, uv).r, 0.0, 1.0);
}

float directionWeight(float3 shadingNormal, float3 sunDirection)
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

public StylizedVoxelShadowSample sampleStylizedVoxelShadow(
    U_GuiInput gui, U_SunInfo sun, U_ShadowCameraInfo shadowCamera,
    Sampler2D terrainShadowMap, Sampler2D leafOpacityMap,
    Sampler2D leafMaskMap, Sampler2D cloudShadowMap,
    float3 voxelCenter, float3 shadingNormal)
{
    float4 worldPosition = float4(voxelCenter, 1.0);
    float terrain = terrainShadowTransmittance(
        terrainShadowMap, shadowCamera.view_proj_mat, worldPosition);
    float leaf = leafShadowTransmittance(
        leafOpacityMap, leafMaskMap, gui, shadowCamera.view_proj_mat, worldPosition);
    float cloud = cloudShadowTransmittance(
        cloudShadowMap, gui, shadowCamera.view_proj_mat, worldPosition);
    StylizedVoxelShadowSample result;
    result.sun_transmittance = terrain * leaf * cloud;
    result.weight =
        result.sun_transmittance * directionWeight(shadingNormal, sun.sun_dir);
    return result;
}

public float stylizedVoxelShadowWeight(
    U_GuiInput gui, U_SunInfo sun, U_ShadowCameraInfo shadowCamera,
    Sampler2D terrainShadowMap, Sampler2D leafOpacityMap,
    Sampler2D leafMaskMap, Sampler2D cloudShadowMap,
    float3 voxelCenter, float3 shadingNormal)
{
    return sampleStylizedVoxelShadow(
        gui, sun, shadowCamera, terrainShadowMap, leafOpacityMap, leafMaskMap,
        cloudShadowMap, voxelCenter, shadingNormal).weight;
}

public float3 applyStylizedVoxelLighting(U_GuiInput gui, U_SunInfo sun,
                                         U_ShadingInfo shading,
                                         float3 baseColor, float shadowWeight,
                                         float3 voxelCenter,
                                         float3 shadingNormal)
{
    float sunLuminance = sunLuminanceFromDirection(sun.sun_dir, sun.sun_luminance);
    float3 sunLight = sun.sun_color * sunLuminance;
    float3 environmentLight = sampleDiffuseEnvironment(
        gui, shading, voxelCenter, shadingNormal);
    return baseColor * (sunLight * shadowWeight + environmentLight);
}
