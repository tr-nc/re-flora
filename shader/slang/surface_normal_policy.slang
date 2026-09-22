module surface_normal_policy;

// The packed-row X-moment implementation is specialized for this smooth radius.
public static const int SURFACE_NORMAL_RADIUS = 2;

// Shared with the CPU tree estimator through build.rs-generated constants.
// Tangential support is in squared voxel units for this radius-two stencil.
public static const float NORMAL_COHERENCE_LOW = 0.05;
public static const float NORMAL_COHERENCE_HIGH = 0.35;
public static const float NORMAL_SUPPORT_LOW = 0.2;
public static const float NORMAL_SUPPORT_HIGH = 1.2;
public static const uint SURFACE_NORMAL_WINDOW_WIDTH =
    uint(SURFACE_NORMAL_RADIUS * 2 + 1);
public static const uint SURFACE_NORMAL_WINDOW_MASK =
    (1u << SURFACE_NORMAL_WINDOW_WIDTH) - 1u;
