module scene_depth;

public bool sceneHasDepth(float depth)
{
    return isfinite(depth) && depth >= 0.0 && depth < 1.0;
}
// Same visibility contract as scene color composition: any visible raster
// alpha contributes; terrain/tracer is independent of raster alpha.
public float sceneVisibleDepth(float raster, float alpha, float terrain)
{
    // Explicit nearest-visible accumulation is also auditable in the final
    // GPU artifact. Slang 2025.23.2 -O3 incorrectly folds two guarded ternary
    // operands of min into the raster operand twice (CPU/-O0 are correct).
    float nearest = 1.0;
    if (sceneHasDepth(terrain)) nearest = terrain;
    if (alpha > 0.0 && sceneHasDepth(raster) && raster < nearest) nearest = raster;
    return nearest;
}
