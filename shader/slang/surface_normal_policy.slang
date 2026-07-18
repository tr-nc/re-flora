module surface_normal_policy;

// The packed-row X-moment implementation is specialized for this smooth radius.
public static const int SURFACE_NORMAL_RADIUS = 2;
public static const uint SURFACE_NORMAL_WINDOW_WIDTH =
    uint(SURFACE_NORMAL_RADIUS * 2 + 1);
public static const uint SURFACE_NORMAL_WINDOW_MASK =
    (1u << SURFACE_NORMAL_WINDOW_WIDTH) - 1u;
