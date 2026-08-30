import ddgi_filter_policy;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    DdgiFilterVisibilitySample nearby = ddgiFilterVisibilitySample(
        2.0, 4.0, 100.0);
    DdgiFilterVisibilitySample backface = ddgiFilterVisibilitySample(
        -10.0, 4.0, 100.0);
    DdgiFilterVisibilitySample distantGeometry = ddgiFilterVisibilitySample(
        8.0, 4.0, 100.0);
    DdgiFilterVisibilitySample atSupportDistance = ddgiFilterVisibilitySample(
        4.0, 4.0, 100.0);
    DdgiFilterVisibilitySample aboveSupportDistance = ddgiFilterVisibilitySample(
        4.001, 4.0, 100.0);
    float skyThreshold = 100.0 * 0.999;
    DdgiFilterVisibilitySample belowSkyThreshold = ddgiFilterVisibilitySample(
        skyThreshold - 0.001, 4.0, 100.0);
    DdgiFilterVisibilitySample atSkyThreshold = ddgiFilterVisibilitySample(
        skyThreshold, 4.0, 100.0);
    DdgiFilterVisibilitySample aboveSkyThreshold = ddgiFilterVisibilitySample(
        skyThreshold + 0.001, 4.0, 100.0);

    DdgiFilterHistoryPolicy noHistory = ddgiFilterHistoryPolicy(
        0u, 1u, false, 0u, 0.9);
    DdgiFilterHistoryPolicy noHistoryRecovery = ddgiFilterHistoryPolicy(
        0u, 1u, true, 0u, 0.9);
    DdgiFilterHistoryPolicy retainedPartition = ddgiFilterHistoryPolicy(
        1u, 1u, false, 2u, 0.9);
    DdgiFilterHistoryPolicy firstRecovery = ddgiFilterHistoryPolicy(
        1u, 1u, true, 0u, 0.9);
    DdgiFilterHistoryPolicy laterRecovery = ddgiFilterHistoryPolicy(
        1u, 1u, true, 2u, 0.9);
    DdgiFilterHistoryPolicy globalUpdate = ddgiFilterHistoryPolicy(
        1u, 0u, false, 0u, 1.5);
    DdgiFilterHistoryPolicy globalRetentionLowerBound = ddgiFilterHistoryPolicy(
        1u, 0u, false, 0u, -0.1);
    bool localBoxPolicy = ddgiFilterProbeNeedsLocalRefresh(
            float3(0.0), 1u, float3(0.0), float3(1.0)) &&
        ddgiFilterProbeNeedsLocalRefresh(
            float3(1.0), 1u, float3(0.0), float3(1.0)) &&
        !ddgiFilterProbeNeedsLocalRefresh(
            float3(1.001), 1u, float3(0.0), float3(1.0)) &&
        !ddgiFilterProbeNeedsLocalRefresh(
            float3(-0.001), 1u, float3(0.0), float3(1.0)) &&
        !ddgiFilterProbeNeedsLocalRefresh(
            float3(0.5), 0u, float3(0.0), float3(1.0));

    bool distancePolicy = nearby.accepted && nearlyEqual(nearby.distance, 2.0) &&
        backface.accepted && nearlyEqual(backface.distance, 2.0) &&
        !distantGeometry.accepted && atSupportDistance.accepted &&
        nearlyEqual(atSupportDistance.distance, 4.0) &&
        !aboveSupportDistance.accepted && !belowSkyThreshold.accepted &&
        atSkyThreshold.accepted && nearlyEqual(atSkyThreshold.distance, 4.0) &&
        aboveSkyThreshold.accepted && nearlyEqual(aboveSkyThreshold.distance, 4.0);
    bool historyPolicy = !noHistory.blend_history && !noHistory.retain_source &&
        !noHistoryRecovery.blend_history && !noHistoryRecovery.retain_source &&
        !retainedPartition.blend_history && retainedPartition.retain_source &&
        firstRecovery.blend_history && !firstRecovery.retain_source &&
        nearlyEqual(firstRecovery.retention, 0.0) &&
        laterRecovery.blend_history && !laterRecovery.retain_source &&
        nearlyEqual(laterRecovery.retention, 2.0 / 3.0) &&
        globalUpdate.blend_history && !globalUpdate.retain_source &&
        nearlyEqual(globalUpdate.retention, 1.0) &&
        globalRetentionLowerBound.blend_history &&
        !globalRetentionLowerBound.retain_source &&
        nearlyEqual(globalRetentionLowerBound.retention, 0.0) &&
        !(noHistory.blend_history && noHistory.retain_source) &&
        !(noHistoryRecovery.blend_history && noHistoryRecovery.retain_source) &&
        !(retainedPartition.blend_history && retainedPartition.retain_source) &&
        !(firstRecovery.blend_history && firstRecovery.retain_source) &&
        !(laterRecovery.blend_history && laterRecovery.retain_source) &&
        !(globalUpdate.blend_history && globalUpdate.retain_source) &&
        !(globalRetentionLowerBound.blend_history &&
          globalRetentionLowerBound.retain_source);
    return distancePolicy && historyPolicy && localBoxPolicy ? 0 : 1;
}
