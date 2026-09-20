import tracer_types;

public struct ButterflyMeshInstance {
    public float4 position_size;
    public float4 color;
    public uint4 metadata;
    public float4 lighting; // x: thin-wing transmission; yzw reserved
};
public struct ButterflyMeshTriangle {
    public float4 a;
    public float4 e1; // xyz: edge; w: wing identity (-1 or +1)
    public float4 e2;
};

// Project a fixed camera-space bounding cube, never fit the current wing pose.
// The tile remains N×N at every distance, while its world footprint is perspective-correct.
public float4 butterflyTileBounds(ButterflyMeshInstance instance, U_CameraInfo camera) {
    float3 center = mul(camera.view_mat, float4(instance.position_size.xyz, 1.0)).xyz;
    float radius = instance.position_size.w * (1.53125 * 0.5);
    float4 nearH=mul(camera.proj_mat_inv,float4(0.0,0.0,0.0,1.0));
    float nearZ=nearH.z/nearH.w;
    float farZ=center.z-radius, frontZ=min(center.z+radius,nearZ);
    if (farZ>=nearZ) return float4(2.0, 2.0, 3.0, 3.0);
    // Clip the conservative cube at the actual near plane before projection.
    // No eye-plane singularity/fullscreen mode switch; keep offscreen bounds
    // intact so moving across a viewport edge does not refit the pixel grid.
    float2 lo = float2(1e20), hi = float2(-1e20);
    [unroll] for (uint i=0; i<8; ++i) {
        float2 xy=center.xy+(float2(i&1u,(i>>1u)&1u)*2.0-1.0)*radius;
        float4 clip = mul(camera.proj_mat,float4(xy,(i&4u)!=0u ? frontZ : farZ,1.0));
        float2 ndc = clip.xy/clip.w;
        lo=min(lo,ndc); hi=max(hi,ndc);
    }
    return float4(lo,hi);
}
public uint butterflyTileAddress(uint instanceIndex, uint2 pixel) {
    return instanceIndex * 4096u + pixel.y * 64u + pixel.x;
}
