import tracer_types;
import model_pixel_grid;

public struct ButterflyMeshInstance {
    public float4 position_size;
    public float4 color;
    public uint4 metadata;
    public float4 lighting; // butterfly: x transmission; leaf (metadata.w & 2): published quaternion
    public uint4 repair; // sparse expression offset/count, always enabled
    public float4 view_orientation; // published rigid pose, excluding articulated wing motion
};
public struct ParticleModelRepairNode {
    public uint4 links; // destination, endpoint refs, lerp float bits
    public float4 colorDepth;
};
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

// Project a fixed camera-space bounding cube, never fit the current wing pose.
// The tile remains N×N at every distance, while its world footprint is perspective-correct.
public float4 butterflyTileBounds(ButterflyMeshInstance instance, U_CameraInfo camera) {
    return modelPixelBounds(instance.position_size.xyz,
        instance.position_size.w*(1.53125*0.5),camera);
}
public uint butterflyTileAddress(uint instanceIndex, uint2 pixel) {
    return instanceIndex * 4096u + pixel.y * 64u + pixel.x;
}
