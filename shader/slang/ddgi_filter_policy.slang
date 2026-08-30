module ddgi_filter_policy;

public struct DdgiFilterVisibilitySample
{
    public bool accepted;
    public float distance;
};

public DdgiFilterVisibilitySample ddgiFilterVisibilitySample(
    float signedDistance, float supportDistance, float farDistanceWorld)
{
    DdgiFilterVisibilitySample result;
    result.accepted = true;
    result.distance = signedDistance < 0.0
        ? abs(signedDistance) * 0.2
        : signedDistance;
    if (result.distance <= supportDistance) return result;

    bool skyMiss = signedDistance >= farDistanceWorld * 0.999;
    result.accepted = skyMiss;
    if (skyMiss) result.distance = supportDistance;
    return result;
}

public bool ddgiFilterProbeNeedsLocalRefresh(
    float3 nominalPosition, uint localRefreshEnabled,
    float3 localRefreshWorldMin, float3 localRefreshWorldMax)
{
    return localRefreshEnabled != 0u &&
        all(nominalPosition >= localRefreshWorldMin) &&
        all(nominalPosition <= localRefreshWorldMax);
}

public struct DdgiFilterHistoryPolicy
{
    public bool blend_history;
    public bool retain_source;
    public float retention;
};

public DdgiFilterHistoryPolicy ddgiFilterHistoryPolicy(
    uint hasHistory, uint localRefreshEnabled, bool localRecoveryProbe,
    uint recoveryEpoch, float configuredRetention)
{
    DdgiFilterHistoryPolicy result;
    bool hasSourceHistory = hasHistory != 0u;
    result.retain_source = hasSourceHistory &&
        localRefreshEnabled != 0u && !localRecoveryProbe;
    result.blend_history = hasSourceHistory && !result.retain_source;
    result.retention = saturate(configuredRetention);
    if (localRecoveryProbe)
    {
        float epoch = float(recoveryEpoch);
        result.retention = epoch / (epoch + 1.0);
    }
    return result;
}
