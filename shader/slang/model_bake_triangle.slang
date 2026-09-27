// Immutable bake input. Never import this from a model-pixel consumer.
module model_bake_triangle;
public struct ButterflyMeshTriangle {
    public float4 a;
    public float4 e1; // xyz: edge; w: wing identity (-1 or +1)
    public float4 e2;
    public float4 normal0;
    public float4 normal1;
    public float4 normal2;
    public float4 uv01;
    public float4 uv2;
};

