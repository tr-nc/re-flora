module glass_surface_sampling;

public static const uint GLASS_VOXEL_FACE_SAMPLE_COUNT = 4u;

// Tensor-product Gauss quadrature over the whole voxel face, in voxel units.
// Two diagonal taps have a nonzero mixed moment and miss the other diagonal.
public float2 glassVoxelFaceSampleOffset(uint index)
{
    float2 side = float2((index & 1u) != 0u ? 1.0 : -1.0,
                         (index & 2u) != 0u ? 1.0 : -1.0);
    return side * 0.2886751346;
}

// Pattern 0 preserves the approved four-point quadrature. Patterns 1..4 are
// explicit lower-cost experiments; no pixel/frame RNG or temporal history.
public uint glassVoxelFaceSampleCount(uint pattern)
{
    if (pattern == 1u || pattern == 2u) return 1u;
    if (pattern == 3u || pattern == 4u) return 2u;
    return GLASS_VOXEL_FACE_SAMPLE_COUNT;
}

public float2 glassVoxelFacePatternOffset(
    uint pattern, uint index, float3 centerRayInFace, float2 screenRightInFace)
{
    if (pattern == 1u) return float2(0.0);
    if (pattern == 2u)
    {
        // Intersect the camera-to-volume-center ray with the canonical face.
        // Keep a strict interior point when another exposed face was selected
        // instead of the camera-dominant face (e.g. alongside an opaque frame).
        if (abs(centerRayInFace.z) <= 1.0e-8) return float2(0.0);
        return clamp(0.5 * centerRayInFace.xy / centerRayInFace.z,
                     float2(-0.49), float2(0.49));
    }
    if (pattern == 3u)
    {
        float squared = dot(screenRightInFace, screenRightInFace);
        float2 axis = squared > 1.0e-12
            ? screenRightInFace * rsqrt(squared) : float2(1.0, 0.0);
        return axis * (index == 0u ? -0.2886751346 : 0.2886751346);
    }
    if (pattern == 4u) return glassVoxelFaceSampleOffset(index * 3u);
    return glassVoxelFaceSampleOffset(index);
}
