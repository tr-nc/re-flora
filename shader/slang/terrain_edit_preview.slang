module terrain_edit_preview;

import tracer_types;

static const float3 PREVIEW_LUMINANCE_WEIGHTS =
    float3(0.2126, 0.7152, 0.0722);
static const float PREVIEW_LUMINANCE_SPLIT = 0.55;
static const float PREVIEW_DARK_LUMINANCE = 0.06;
static const float PREVIEW_LIGHT_LUMINANCE = 1.05;

public float terrainEditPreviewTint(U_TerrainEditPreview preview, float3 centerPosition)
{
    if (preview.enabled == 0u || preview.radius <= 0.0) return 0.0;

    float distance = preview.shape == 1u
        ? length(centerPosition.xz - preview.center.xz)
        : length(centerPosition - preview.center);
    // A narrow shell communicates the brush extent without washing the edited area.
    // Keep at least three terrain voxels across the full ring so it remains legible
    // for the smallest brush and under half-resolution internal rendering.
    float ringHalfWidth = max(preview.radius * 0.06, 1.5 / 256.0);
    float distanceToRing = abs(distance - preview.radius);
    float ringMask = 1.0 - smoothstep(
        ringHalfWidth * 0.55, ringHalfWidth, distanceToRing);
    return clamp(ringMask * preview.strength, 0.0, 1.0);
}

float3 terrainEditPreviewContrastColor(float3 baseColor, float3 accentColor)
{
    // The preview is applied before tone mapping. Remap to a fixed scene-linear
    // luminance on the opposite side of the base instead of adding brightness;
    // this preserves contrast on both sunlit pale surfaces and dark terrain.
    float baseLuminance = dot(max(baseColor, float3(0.0)),
                              PREVIEW_LUMINANCE_WEIGHTS);
    float targetLuminance = baseLuminance >= PREVIEW_LUMINANCE_SPLIT
        ? PREVIEW_DARK_LUMINANCE
        : PREVIEW_LIGHT_LUMINANCE;
    float accentLuminance = max(
        dot(max(accentColor, float3(0.0)), PREVIEW_LUMINANCE_WEIGHTS),
        1.0e-4);
    return max(accentColor, float3(0.0)) *
           (targetLuminance / accentLuminance);
}

public float3 applyTerrainEditPreviewTint(U_TerrainEditPreview preview,
                                          float3 baseColor, float3 centerPosition)
{
    float tint = terrainEditPreviewTint(preview, centerPosition);
    if (tint <= 0.0) return baseColor;
    float3 contrastColor = terrainEditPreviewContrastColor(
        baseColor, preview.color);
    return lerp(baseColor, contrastColor, tint);
}
