module ddgi_visibility_filter_owner;

import ddgi_filter_evidence;
import ddgi_filter_execution;
import ddgi_filter_policy;

[[vk::binding(28, 0)]]
RWTexture2D<float2> ddgi_visibility_atlas;

[[vk::binding(39, 0)]]
RWTexture2D<float2> ddgi_transport_source_visibility_atlas;

public struct DdgiOwnedVisibilityAccumulator
{
    float weightedDistance;
    float weightedSquaredDistance;
    float accumulatedWeight;
};

public DdgiOwnedVisibilityAccumulator ddgiOwnerBeginVisibility()
{
    DdgiOwnedVisibilityAccumulator result;
    result.weightedDistance = 0.0;
    result.weightedSquaredDistance = 0.0;
    result.accumulatedWeight = 0.0;
    return result;
}

public void ddgiOwnerAccumulateVisibilitySample(
    uint evidenceEnabled, bool electedEvidenceLane,
    DdgiFilterVisibilityDecision decision, float weight,
    inout DdgiOwnedVisibilityAccumulator accumulator)
{
    ddgiRecordVisibilitySampleDecision(
        evidenceEnabled, electedEvidenceLane, decision);
    ddgiAccumulateVisibility(
        decision, weight, accumulator.weightedDistance,
        accumulator.weightedSquaredDistance, accumulator.accumulatedWeight);
}

public float2 ddgiOwnerFinishVisibility(
    DdgiOwnedVisibilityAccumulator accumulator, float supportDistance)
{
    if (accumulator.accumulatedWeight <= 1.0e-8)
        return float2(supportDistance, supportDistance * supportDistance);
    return float2(
        accumulator.weightedDistance, accumulator.weightedSquaredDistance) /
        accumulator.accumulatedWeight;
}

public float2 ddgiOwnerLoadVisibility(uint slot, uint2 coordinate)
{
    return slot == 0u
        ? ddgi_visibility_atlas[coordinate]
        : ddgi_transport_source_visibility_atlas[coordinate];
}

void ddgiOwnerStoreVisibilityValue(
    uint destinationSlot, uint2 coordinate, float2 value)
{
    if (destinationSlot == 0u)
        ddgi_visibility_atlas[coordinate] = value;
    else
        ddgi_transport_source_visibility_atlas[coordinate] = value;
}

public bool ddgiOwnerTryStoreTerminalVisibility(
    uint evidenceEnabled, bool electedProbeLane,
    uint destinationSlot, uint2 coordinate,
    DdgiFilterHistoryDecision requested, bool freshAvailable, float2 source)
{
    if (requested.action != DdgiFilterHistoryAction::Retain && freshAvailable)
        return false;
    DdgiExecutedHistory2 executed = ddgiExecuteHistory(
        requested, freshAvailable, float2(0.0), source);
    ddgiRecordVisibilityHistoryDecision(
        evidenceEnabled, electedProbeLane, executed.decision);
    ddgiOwnerStoreVisibilityValue(
        destinationSlot, coordinate, executed.value);
    return true;
}

public void ddgiOwnerStoreFreshVisibility(
    uint evidenceEnabled, bool electedProbeLane,
    uint destinationSlot, uint2 coordinate,
    DdgiFilterHistoryDecision requested, float2 fresh, float2 source)
{
    DdgiExecutedHistory2 executed = ddgiExecuteHistory(
        requested, true, fresh, source);
    ddgiRecordVisibilityHistoryDecision(
        evidenceEnabled, electedProbeLane, executed.decision);
    ddgiOwnerStoreVisibilityValue(destinationSlot, coordinate, executed.value);
}
