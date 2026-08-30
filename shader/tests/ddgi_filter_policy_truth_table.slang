import ddgi_filter_policy;
import ddgi_filter_execution;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    DdgiFilterVisibilityDecision nearby = ddgiFilterVisibilityDecision(
        2.0, 4.0, 100.0);
    DdgiFilterVisibilityDecision backface = ddgiFilterVisibilityDecision(
        -10.0, 4.0, 100.0);
    DdgiFilterVisibilityDecision distantGeometry = ddgiFilterVisibilityDecision(
        8.0, 4.0, 100.0);
    float skyThreshold = 100.0 * 0.999;
    DdgiFilterVisibilityDecision belowSkyThreshold = ddgiFilterVisibilityDecision(
        skyThreshold - 0.001, 4.0, 100.0);
    DdgiFilterVisibilityDecision atSkyThreshold = ddgiFilterVisibilityDecision(
        skyThreshold, 4.0, 100.0);
    DdgiFilterVisibilityDecision aboveSkyThreshold = ddgiFilterVisibilityDecision(
        skyThreshold + 0.001, 4.0, 100.0);

    DdgiFilterHistoryDecision noHistory = ddgiFilterHistoryDecision(
        0u, 1u, false, 0u, 0.9);
    DdgiFilterHistoryDecision noHistoryRecovery = ddgiFilterHistoryDecision(
        0u, 1u, true, 0u, 0.9);
    DdgiFilterHistoryDecision retainedPartition = ddgiFilterHistoryDecision(
        1u, 1u, false, 2u, 0.9);
    DdgiFilterHistoryDecision firstRecovery = ddgiFilterHistoryDecision(
        1u, 1u, true, 0u, 0.9);
    DdgiFilterHistoryDecision laterRecovery = ddgiFilterHistoryDecision(
        1u, 1u, true, 2u, 0.9);
    DdgiFilterHistoryDecision configuredCapRecovery = ddgiFilterHistoryDecision(
        1u, 1u, true, 8u, 0.25);
    DdgiFilterHistoryDecision globalUpdate = ddgiFilterHistoryDecision(
        1u, 0u, false, 0u, 1.5);
    bool localBoxPolicy = ddgiFilterProbeNeedsLocalRefresh(
            float3(1.0), 1u, float3(0.0), float3(1.0)) &&
        !ddgiFilterProbeNeedsLocalRefresh(
            float3(1.001), 1u, float3(0.0), float3(1.0)) &&
        !ddgiFilterProbeNeedsLocalRefresh(
            float3(0.5), 0u, float3(0.0), float3(1.0));

    bool distancePolicy = nearby.owner_version == DDGI_FILTER_POLICY_OWNER_VERSION &&
        nearby.action == DdgiFilterVisibilityAction::Accept &&
        nearlyEqual(nearby.distance, 2.0) &&
        backface.action == DdgiFilterVisibilityAction::Accept &&
        nearlyEqual(backface.distance, 2.0) &&
        distantGeometry.action == DdgiFilterVisibilityAction::Reject &&
        belowSkyThreshold.action == DdgiFilterVisibilityAction::Reject &&
        atSkyThreshold.action == DdgiFilterVisibilityAction::Accept &&
        nearlyEqual(atSkyThreshold.distance, 4.0) &&
        aboveSkyThreshold.action == DdgiFilterVisibilityAction::Accept &&
        nearlyEqual(aboveSkyThreshold.distance, 4.0);
    bool historyPolicy =
        noHistory.owner_version == DDGI_FILTER_POLICY_OWNER_VERSION &&
        noHistory.action == DdgiFilterHistoryAction::Replace &&
        noHistoryRecovery.action == DdgiFilterHistoryAction::Replace &&
        retainedPartition.action == DdgiFilterHistoryAction::Retain &&
        firstRecovery.action == DdgiFilterHistoryAction::Blend &&
        nearlyEqual(firstRecovery.retention, 0.0) &&
        laterRecovery.action == DdgiFilterHistoryAction::Blend &&
        nearlyEqual(laterRecovery.retention, 2.0 / 3.0) &&
        configuredCapRecovery.action == DdgiFilterHistoryAction::Blend &&
        nearlyEqual(configuredCapRecovery.retention, 0.25) &&
        globalUpdate.action == DdgiFilterHistoryAction::Blend &&
        nearlyEqual(globalUpdate.retention, 1.0);

    float4 fresh = float4(2.0, 4.0, 6.0, 1.0);
    float4 source = float4(10.0, 12.0, 14.0, 1.0);
    DdgiExecutedHistory4 replacedFresh = ddgiExecuteHistory(
        noHistory, true, fresh, source);
    DdgiExecutedHistory4 retainedFresh = ddgiExecuteHistory(
        retainedPartition, true, fresh, source);
    DdgiExecutedHistory4 invalidFresh = ddgiExecuteHistory(
        laterRecovery, false, fresh, source);
    DdgiExecutedHistory4 blendedFresh = ddgiExecuteHistory(
        ddgiFilterHistoryDecision(1u, 1u, true, 1u, 0.9),
        true, fresh, source);
    bool historyExecution =
        all(replacedFresh.value == fresh) &&
        replacedFresh.decision.action == DdgiFilterHistoryAction::Replace &&
        all(retainedFresh.value == source) &&
        retainedFresh.decision.action == DdgiFilterHistoryAction::Retain &&
        all(blendedFresh.value == float4(6.0, 8.0, 10.0, 1.0)) &&
        blendedFresh.decision.action == DdgiFilterHistoryAction::Blend &&
        all(invalidFresh.value == float4(0.0)) &&
        invalidFresh.decision.action == DdgiFilterHistoryAction::Replace &&
        nearlyEqual(invalidFresh.decision.retention, 0.0);

    float weightedDistance = 3.0;
    float weightedSquaredDistance = 9.0;
    float accumulatedWeight = 1.5;
    ddgiAccumulateVisibility(
        nearby, 0.5, weightedDistance, weightedSquaredDistance,
        accumulatedWeight);
    bool acceptedExecution = nearlyEqual(weightedDistance, 4.0) &&
        nearlyEqual(weightedSquaredDistance, 11.0) &&
        nearlyEqual(accumulatedWeight, 2.0);
    ddgiAccumulateVisibility(
        distantGeometry, 100.0, weightedDistance, weightedSquaredDistance,
        accumulatedWeight);
    bool rejectedExecution = nearlyEqual(weightedDistance, 4.0) &&
        nearlyEqual(weightedSquaredDistance, 11.0) &&
        nearlyEqual(accumulatedWeight, 2.0);

    return distancePolicy && historyPolicy && localBoxPolicy &&
        historyExecution && acceptedExecution && rejectedExecution ? 0 : 1;
}
