module terrain_edit_preview;

import tracer_types;

public float terrainEditPreviewTint(U_TerrainEditPreview preview, float3 centerPosition)
{
    if (preview.enabled == 0u || preview.radius <= 0.0) return 0.0;

    float distance = preview.shape == 1u || preview.shape == 2u
        ? length(centerPosition.xz - preview.center.xz)
        : length(centerPosition - preview.center);
    float edge = max(preview.radius * 0.12, 0.002);
    float mask = 1.0 - smoothstep(preview.radius - edge, preview.radius, distance);
    return clamp(mask * preview.strength, 0.0, 1.0);
}

public float3 applyTerrainEditPreviewTint(U_TerrainEditPreview preview,
                                          float3 baseColor, float3 centerPosition)
{
    float tint = terrainEditPreviewTint(preview, centerPosition);
    return tint > 0.0 ? lerp(baseColor, preview.color, tint) : baseColor;
}
