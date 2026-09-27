// Cache ABI and source identities, independent of geometry generation.
module model_cache_types;
public struct ModelBakedSurface {
    public float4 positionDepth;
    public float4 normalMaterial;
};
public static const uint APPLE_MODEL_SOURCE=64u;
public uint modelSourceIndex(uint kind,uint shape) {
    return kind==0u?shape:kind==1u?APPLE_MODEL_SOURCE:kind==2u?65u+shape:97u+shape;
}
