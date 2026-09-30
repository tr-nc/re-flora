// Cache ABI and source identities, independent of geometry generation.
module model_cache_types;
public struct ModelBakedSurface {
    public float4 positionDepth;
    public float4 normalMaterial;
};
// Flower UVs use two UNORM12 coordinates in one exactly representable float
// integer (24 bits). No NaN bit patterns, no change to the cached-surface ABI.
public float encodeFlowerUV(float2 uv) {
    uint2 q=uint2(floor(saturate(uv)*4095.0+0.5));
    return float(q.x|(q.y<<12u));
}
public float2 decodeFlowerUV(float encoded) {
    uint q=uint(encoded);
    return float2(q&4095u,(q>>12u)&4095u)/4095.0;
}
public static const uint APPLE_MODEL_SOURCE=64u;
public uint modelSourceIndex(uint kind,uint shape) {
    return kind==0u?shape:kind==1u?APPLE_MODEL_SOURCE:kind==2u?65u+shape:97u+shape;
}
