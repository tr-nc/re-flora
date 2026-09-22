module occupancy_normal;

import surface_normal_policy;

// Unnormalised evidence from the radius-two occupancy stencil. Independent of
// material, render path and camera. Packed rows preserve the original estimator.
public struct OccupancyNormalMoments
{
    float3 first;
    float3 diagonal;
    float3 crossTerms; // xy, xz, yz
    float count;
    float distanceSum;
};

public OccupancyNormalMoments beginOccupancyNormal()
{
    OccupancyNormalMoments result;
    result.first = float3(0);
    result.diagonal = float3(0);
    result.crossTerms = float3(0);
    result.count = 0.0;
    result.distanceSum = 0.0;
    return result;
}

public void accumulateOccupancyRow(uint window, int y, int z,
    inout OccupancyNormalMoments moments)
{
    float outerNegative = float(window & 1u);
    float innerNegative = float((window >> 1u) & 1u);
    float middle = float((window >> 2u) & 1u);
    float innerPositive = float((window >> 3u) & 1u);
    float outerPositive = float((window >> 4u) & 1u);
    float count = float(countbits(window));
    float x = 2.0 * (outerPositive - outerNegative) + innerPositive - innerNegative;
    float xSquared = 4.0 * (outerPositive + outerNegative) + innerPositive + innerNegative;
    moments.first += float3(x, y * count, z * count);
    moments.diagonal += float3(xSquared, y*y*count, z*z*count);
    moments.crossTerms += float3(x*y, x*z, y*z*count);
    moments.count += count;
    float yzSquared = float(y*y + z*z);
    moments.distanceSum += (outerPositive + outerNegative) * sqrt(yzSquared + 4.0)
        + (innerPositive + innerNegative) * sqrt(yzSquared + 1.0)
        + middle * sqrt(yzSquared);
}

float3 covarianceProduct(float3 diagonal, float3 crossTerms, float3 v)
{
    return diagonal * v + float3(
        crossTerms.x*v.y + crossTerms.y*v.z,
        crossTerms.x*v.x + crossTerms.z*v.z,
        crossTerms.y*v.x + crossTerms.z*v.y);
}

public float4 finishOccupancyNormal(OccupancyNormalMoments moments)
{
    float lengthSquared = dot(moments.first, moments.first);
    if (lengthSquared == 0.0) return float4(0,1,0,0);
    float3 normal = -moments.first * rsqrt(lengthSquared);
    float coherence = sqrt(lengthSquared) / max(moments.distanceSum, 1.0e-8);
    float3 mean = moments.first / moments.count;
    float3 diagonal = moments.diagonal / moments.count - mean*mean;
    float3 crossTerms = moments.crossTerms / moments.count - mean.xxy * mean.yzz;
    float3 u = normalize(cross(normal, abs(normal.x) < 0.9 ? float3(1,0,0) : float3(0,1,0)));
    float3 v = cross(normal, u);
    float3 cu = covarianceProduct(diagonal, crossTerms, u);
    float3 cv = covarianceProduct(diagonal, crossTerms, v);
    float a = dot(u, cu);
    float b = dot(u, cv);
    float d = dot(v, cv);
    float support = max(0.0, 0.5 * (a+d - sqrt((a-d)*(a-d) + 4.0*b*b)));
    float confidence = smoothstep(NORMAL_COHERENCE_LOW, NORMAL_COHERENCE_HIGH, coherence)
        * smoothstep(NORMAL_SUPPORT_LOW, NORMAL_SUPPORT_HIGH, support);
    return float4(normal, confidence);
}
