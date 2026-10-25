module grass_growth_shape;

// Geometry and palette share the current discrete visible height.
public float grassGrownHeight(uint heightVoxels, float growthFactor)
{
    return floor(float(heightVoxels) * growthFactor + 0.001);
}

public float grassColorGradient(int layer, float visibleHeight)
{
    // A single visible layer is also the tip. Empty stems are trimmed by geometry.
    if (visibleHeight <= 1.0) return 1.0;
    return clamp(float(layer) / (visibleHeight - 1.0), 0.0, 1.0);
}
