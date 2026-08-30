module ddgi_filter_execution;

import ddgi_filter_policy;

public struct DdgiExecutedHistory4
{
    public DdgiFilterHistoryDecision decision;
    public float4 value;
};

public struct DdgiExecutedHistory2
{
    public DdgiFilterHistoryDecision decision;
    public float2 value;
};

public DdgiExecutedHistory4 ddgiExecuteHistory(
    DdgiFilterHistoryDecision requested, bool freshAvailable,
    float4 fresh, float4 source)
{
    DdgiExecutedHistory4 result;
    result.decision = requested;
    if (requested.action == DdgiFilterHistoryAction::Retain)
    {
        result.value = source;
        return result;
    }
    if (!freshAvailable)
    {
        result.decision.action = DdgiFilterHistoryAction::Replace;
        result.decision.retention = 0.0;
        result.value = float4(0.0);
        return result;
    }
    result.value = requested.action == DdgiFilterHistoryAction::Blend
        ? float4(lerp(fresh.xyz, source.xyz, requested.retention), fresh.w)
        : fresh;
    return result;
}

public DdgiExecutedHistory2 ddgiExecuteHistory(
    DdgiFilterHistoryDecision requested, bool freshAvailable,
    float2 fresh, float2 source)
{
    DdgiExecutedHistory2 result;
    result.decision = requested;
    if (requested.action == DdgiFilterHistoryAction::Retain)
    {
        result.value = source;
        return result;
    }
    if (!freshAvailable)
    {
        result.decision.action = DdgiFilterHistoryAction::Replace;
        result.decision.retention = 0.0;
        result.value = float2(0.0);
        return result;
    }
    result.value = requested.action == DdgiFilterHistoryAction::Blend
        ? lerp(fresh, source, requested.retention)
        : fresh;
    return result;
}

// Owner-owned execution is deliberately independent of capture and evidence schemas. Production
// filters pass one typed policy decision here and store the returned value unchanged.
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
