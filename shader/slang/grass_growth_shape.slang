module grass_growth_shape;

// Geometry and palette share the same discrete, competition-limited height.
public float grassGrownHeight(uint heightVoxels, float growthFactor)
{
    return floor(float(heightVoxels) * growthFactor + 0.001);
}

public float grassColorGradient(int layer, uint heightVoxels, float growthPotential)
{
    float matureHeight = grassGrownHeight(heightVoxels, growthPotential);
    // A single visible layer is also the tip. Empty stems are trimmed by geometry.
    if (matureHeight <= 1.0) return 1.0;
    return clamp(float(layer) / (matureHeight - 1.0), 0.0, 1.0);
}
