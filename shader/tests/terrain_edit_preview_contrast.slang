import terrain_edit_preview;
import tracer_types;

// Compile this as a CPU executable so the assertions exercise the production
// preview shader instead of a duplicated approximation:
// slangc shader/tests/terrain_edit_preview_contrast.slang -I shader/slang \
//   -target executable -o /tmp/re-flora-preview-contrast

float luminance(float3 color)
{
    return dot(color, float3(0.2126, 0.7152, 0.0722));
}

bool nearlyEqual(float3 a, float3 b)
{
    return all(abs(a - b) < float3(1.0e-5));
}

export __extern_cpp int main()
{
    U_TerrainEditPreview preview = {};
    preview.center = float3(0.0);
    preview.radius = 1.0;
    preview.color = float3(0.45, 0.86, 1.0);
    preview.strength = 0.9;
    preview.enabled = 1u;
    preview.shape = 1u;

    float3 ringPosition = float3(preview.radius, 0.0, 0.0);
    float3 brightSurface = float3(1.0);
    float3 darkSurface = float3(0.02);
    float3 brightMapped = applyTerrainEditPreviewTint(
        preview, brightSurface, ringPosition);
    float3 darkMapped = applyTerrainEditPreviewTint(
        preview, darkSurface, ringPosition);
    float brightContrast = abs(
        luminance(brightMapped) - luminance(brightSurface));
    float darkContrast = abs(
        luminance(darkMapped) - luminance(darkSurface));

    float3 centerMapped = applyTerrainEditPreviewTint(
        preview, brightSurface, preview.center);
    float3 outsideMapped = applyTerrainEditPreviewTint(
        preview, brightSurface, float3(1.5, 0.0, 0.0));

    printf("bright_contrast=%.6f dark_contrast=%.6f center_unchanged=%d outside_unchanged=%d\n",
           brightContrast, darkContrast,
           nearlyEqual(centerMapped, brightSurface),
           nearlyEqual(outsideMapped, brightSurface));

    bool contrastPass = brightContrast >= 0.65 && darkContrast >= 0.65;
    bool ringPass = nearlyEqual(centerMapped, brightSurface) &&
                    nearlyEqual(outsideMapped, brightSurface);
    return contrastPass && ringPass ? 0 : 1;
}
