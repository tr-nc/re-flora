module ddgi_filter_policy;

public static const uint DDGI_FILTER_POLICY_OWNER_VERSION = 1u;

public enum DdgiFilterVisibilityAction : uint
{
    Accept = 0u,
    Reject = 1u,
};

public struct DdgiFilterVisibilityDecision
{
    public uint owner_version;
    public DdgiFilterVisibilityAction action;
    public float distance;
};

public DdgiFilterVisibilityDecision ddgiFilterVisibilityDecision(
    float signedDistance, float supportDistance, float farDistanceWorld)
{
    DdgiFilterVisibilityDecision result;
    result.owner_version = DDGI_FILTER_POLICY_OWNER_VERSION;
    result.action = DdgiFilterVisibilityAction::Accept;
    result.distance = signedDistance < 0.0
        ? abs(signedDistance) * 0.2
        : signedDistance;
    if (result.distance <= supportDistance) return result;

    bool skyMiss = signedDistance >= farDistanceWorld * 0.999;
    result.action = skyMiss
        ? DdgiFilterVisibilityAction::Accept
        : DdgiFilterVisibilityAction::Reject;
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

public enum DdgiFilterHistoryAction : uint
{
    Replace = 0u,
    Retain = 1u,
    Blend = 2u,
};

public struct DdgiFilterHistoryDecision
{
    public uint owner_version;
    public DdgiFilterHistoryAction action;
    public float retention;
};

public DdgiFilterHistoryDecision ddgiFilterHistoryDecision(
    uint hasHistory, uint localRefreshEnabled, bool localRecoveryProbe,
    uint recoveryEpoch, float configuredRetention)
{
    DdgiFilterHistoryDecision result;
    result.owner_version = DDGI_FILTER_POLICY_OWNER_VERSION;
    bool hasSourceHistory = hasHistory != 0u;
    result.action = DdgiFilterHistoryAction::Replace;
    if (hasSourceHistory)
    {
        result.action = localRefreshEnabled != 0u && !localRecoveryProbe
            ? DdgiFilterHistoryAction::Retain
            : DdgiFilterHistoryAction::Blend;
    }
    result.retention = saturate(configuredRetention);
    if (localRecoveryProbe)
    {
        float epoch = float(recoveryEpoch);
        result.retention = epoch / (epoch + 1.0);
    }
    return result;
}
