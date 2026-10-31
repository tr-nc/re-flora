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
