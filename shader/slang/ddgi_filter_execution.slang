module ddgi_filter_execution;

import ddgi_filter_policy;

// Owner-owned execution is deliberately independent of capture and evidence schemas. Production
// filters pass one typed policy decision here and store the returned value unchanged.
public float4 ddgiCommitHistory(
    DdgiFilterHistoryDecision decision, float4 fresh, float4 source)
{
    switch (decision.action)
    {
    case DdgiFilterHistoryAction::Retain:
        return source;
    case DdgiFilterHistoryAction::Blend:
        return float4(
            lerp(fresh.xyz, source.xyz, decision.retention), fresh.w);
    default:
        return fresh;
    }
}

public float2 ddgiCommitHistory(
    DdgiFilterHistoryDecision decision, float2 fresh, float2 source)
{
    switch (decision.action)
    {
    case DdgiFilterHistoryAction::Retain:
        return source;
    case DdgiFilterHistoryAction::Blend:
        return lerp(fresh, source, decision.retention);
    default:
        return fresh;
    }
}

public void ddgiAccumulateVisibility(
    DdgiFilterVisibilityDecision decision, float weight,
    inout float weightedDistance, inout float weightedSquaredDistance,
    inout float accumulatedWeight)
{
    if (decision.action == DdgiFilterVisibilityAction::Reject) return;
    weightedDistance += decision.distance * weight;
    weightedSquaredDistance += decision.distance * decision.distance * weight;
    accumulatedWeight += weight;
}
