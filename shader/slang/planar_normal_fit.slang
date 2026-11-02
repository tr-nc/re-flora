module planar_normal_fit;

// Local least-squares fit h = a*u + b*v + c, in voxel units. Inputs are
// visible surface intersections, not a procedural wall recipe or saved geometry.
public struct PlanarNormalFit
{
    float count;
    float3 sum; // u, v, h
    float3 square;
    float3 crossTerms; // uv, uh, vh
};

public PlanarNormalFit beginPlanarNormalFit()
{
    PlanarNormalFit fit;
    fit.count = 0.0;
    fit.sum = float3(0);
    fit.square = float3(0);
    fit.crossTerms = float3(0);
    return fit;
}

public void addPlanarNormalPoint(inout PlanarNormalFit fit, float3 point)
{
    fit.count += 1.0;
    fit.sum += point;
    fit.square += point * point;
    fit.crossTerms += point.xxy * point.yzz;
}

public bool finishPlanarNormalFit(PlanarNormalFit fit, out float2 slope)
{
    slope = float2(0);
    if (fit.count < 6.0) return false;
    float3 variance = fit.square - fit.sum * fit.sum / fit.count;
    float3 covariance = fit.crossTerms - fit.sum.xxy * fit.sum.yzz / fit.count;
    float determinant = variance.x * variance.y - covariance.x * covariance.x;
    // Require genuinely two-dimensional support, including partial stencils at
    // material boundaries. Lines and tiny patches cannot establish a plane.
    if (min(variance.x, variance.y) < 72.0 || determinant < 1024.0) return false;
    slope = float2(
        variance.y * covariance.y - covariance.x * covariance.z,
        variance.x * covariance.z - covariance.x * covariance.y) / determinant;
    float residual = max(0.0, variance.z - dot(slope, covariance.yz)) / fit.count;
    // A voxelized plane has up to half a voxel of quantization error. Reject
    // folds, corners and displaced geometry instead of rounding their features.
    return residual <= 0.45 * 0.45;
}
