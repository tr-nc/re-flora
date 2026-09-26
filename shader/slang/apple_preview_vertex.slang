// Packed shared apple model positions for the attached-fruit shadow pass.
module apple_preview_vertex;
public float3 applePreviewPosition(uint packed) {
    uint3 raw=uint3(packed & 511u,(packed>>9u)&511u,(packed>>18u)&511u);
    return (float3(raw)-128.0)/16.0;
}
