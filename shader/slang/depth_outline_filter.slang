module depth_outline_filter;

public float outlineVisibleDepth(float raster, float alpha, float terrain)
{
    bool hasTerrain = isfinite(terrain) && terrain >= 0.0 && terrain < 1.0;
    bool hasOpaqueRaster = isfinite(raster) && raster >= 0.0 && raster < 1.0 && alpha >= 0.999;
    return min(hasTerrain ? terrain : 1.0, hasOpaqueRaster ? raster : 1.0);
}

// Reciprocal eye depth is affine across a projected plane, unlike eye depth.
// The opposing-gradient residual rejects slopes without a normal texture.
// c > 0 is foreground; zero denotes sky/invalid samples, never infinity.
public float outlinePair(float c, float a, float b, float relativeThreshold,
                         float minimumGap, float softness, float skyStrength)
{
    if (c <= 0.0 || !isfinite(c) || !isfinite(a) || !isfinite(b)) return 0.0;
    float outward = max(c - a, c - b);
    if (outward <= 0.0) return 0.0; // no halo on the farther/background side
    float residual = max(0.0, 2.0 * c - a - b) / c;
    float limit = max(relativeThreshold, minimumGap * c);
    float edge = smoothstep(limit, limit * (1.0 + softness), residual);
    if (a == 0.0 || b == 0.0) edge *= skyStrength;
    return edge;
}
public bool outlineThinPair(float c, float a, float b, float threshold)
{
    return c > 0.0 && min(c - a, c - b) / c > threshold;
}
public float outlineCoverage(float signal, uint thinPairs, float thinStrength)
{
    // Several opposing gaps indicate subpixel/one-pixel foliage, not a broad
    // silhouette. Attenuate it rather than filling every tiny leaf with ink.
    return signal * (thinPairs >= 2u ? thinStrength : 1.0);
}
public float outlineInverseEyeDepth(float deviceDepth, float2 uv, float4x4 inverseProjection)
{
    if (!isfinite(deviceDepth) || deviceDepth < 0.0 || deviceDepth >= 1.0) return 0.0;
    float4 view = mul(inverseProjection, float4(uv * 2.0 - 1.0, deviceDepth, 1.0));
    if (!all(isfinite(view)) || abs(view.z) < 1e-8 || abs(view.w) < 1e-8) return 0.0;
    return abs(view.w / view.z);
}
