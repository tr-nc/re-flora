module depth_outline_filter;

public struct OutlineSample {
    public float inverseDepth;
    public float3 viewPosition;
};
public OutlineSample outlineSample(float deviceDepth, float2 uv, float4x4 inverseProjection)
{
    OutlineSample result = {0.0, float3(0.0)};
    if (!isfinite(deviceDepth) || deviceDepth < 0.0 || deviceDepth >= 1.0) return result;
    float4 view = mul(inverseProjection, float4(uv * 2.0 - 1.0, deviceDepth, 1.0));
    if (!all(isfinite(view)) || abs(view.z) < 1e-8 || abs(view.w) < 1e-8) return result;
    result.inverseDepth = abs(view.w / view.z);
    result.viewPosition = view.xyz / view.w;
    return result;
}
// Two samples on each side distinguish a discontinuity from a continuous
// fold. Inverse depth is affine on projected planes, so extrapolation across
// a plane predicts the center exactly, regardless of slope or distance.
public float outlinePair(OutlineSample center, OutlineSample a, OutlineSample b,
                         float outerA, float outerB, float4 thresholds, float skyStrength)
{
    float c = center.inverseDepth;
    if (c <= 0.0) return 0.0;
    float errorA = (c - 2.0 * a.inverseDepth + outerA) / c;
    float errorB = (c - 2.0 * b.inverseDepth + outerB) / c;
    float limit = max(thresholds.x, thresholds.y * c);
    float edge = smoothstep(limit, limit * (1.0 + thresholds.z), max(errorA, errorB));
    bool sky = a.inverseDepth == 0.0 || b.inverseDepth == 0.0;
    if (sky) return edge * skyStrength;
    float3 left = center.viewPosition - a.viewPosition;
    float3 right = b.viewPosition - center.viewPosition;
    float lengths = dot(left,left) * dot(right,right);
    if (lengths <= 1e-20) return 0.0;
    float turn = 1.0 - clamp(dot(left,right) * rsqrt(lengths), -1.0, 1.0);
    // Outer samples can cross a fold one pixel away. They must not dilate
    // that fold onto a center whose immediate neighbours are still planar.
    if (turn < 1e-5) return 0.0;
    // A discontinuity on the farther side must not produce a second halo.
    // Continuous folds have zero extrapolation error on both sides, including
    // concave folds previously discarded by the signed three-tap residual.
    // Continuity is a geometric guard, not an artistic jump threshold. A
    // large saved threshold must not turn unrelated overlapping leaf depths
    // into continuous folds or enable background-side crease halos.
    float continuityLimit = min(limit, 0.01);
    if (max(abs(errorA), abs(errorB)) <= continuityLimit) {
        float turnLimit = max(1.0 - thresholds.w, 1e-5);
        edge = max(edge, smoothstep(turnLimit, turnLimit * (1.0 + thresholds.z), turn));
    }
    return edge;
}
public bool outlineThinPair(float c, float a, float b, float threshold)
{
    return c > 0.0 && min(c - a, c - b) / c > threshold;
}
public float outlineCoverage(float signal, uint thinPairs, float thinStrength)
{
    return signal * (thinPairs >= 2u ? thinStrength : 1.0);
}
