import stem_band_geometry;
[shader("fragment")]
float4 main(StemBandVertex input):SV_Target0 {
    // Flat final lighting/color, not only a flat albedo. Raster depth remains
    // hardware-generated; there is no per-pixel intersection or SV_Depth.
    return float4(input.color,1.0);
}
