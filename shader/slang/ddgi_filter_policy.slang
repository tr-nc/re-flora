module ddgi_filter_policy;

import ddgi_types;

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
        result.retention = min(result.retention, epoch / (epoch + 1.0));
    }
    return result;
}

public DdgiFilterHistoryDecision ddgiFilterIrradianceHistoryDecision(
    uint hasHistory, uint recoveryEnabled, uint recoveryEpoch,
    float configuredRetention)
{
    // A geometry-local bound justifies retaining distant visibility moments, not radiance.
    // Incoming light can change anywhere through remote bounce paths or light mutations.
    // Consume the already-traced transport across the whole field, applying the same
    // field-epoch recovery cap everywhere, even when new edits repeatedly restart e0.
    return ddgiFilterHistoryDecision(hasHistory, 0u, recoveryEnabled != 0u,
        recoveryEpoch, min(configuredRetention, 0.99));
}

public bool ddgiAggregatePlacementMatches(DdgiProbeMetadata current, DdgiProbeMetadata source)
{
    return current.state_and_reserved.x == DDGI_PROBE_STATE_VALID &&
        source.state_and_reserved.x == DDGI_PROBE_STATE_VALID &&
        all(current.nominal_position_and_min_clearance == source.nominal_position_and_min_clearance) &&
        all(current.actual_position_and_clearance == source.actual_position_and_clearance);
}

// Aggregate history is an approximate prior, not a cache of individually validated rays.
// Exact unchanged placement and clearance, with edits outside that clearance region, qualify
// it for bounded blending only. First-segment evidence never implies unchanged radiance:
// every probe incorporates fresh current-visibility transport, including the remote partition.
public DdgiFilterHistoryDecision ddgiAggregateHistoryDecision(
    DdgiProbeMetadata current, DdgiProbeMetadata source,
    float3 editMin, float3 editMax, float retention)
{
    DdgiFilterHistoryDecision result;
    result.owner_version = DDGI_FILTER_POLICY_OWNER_VERSION;
    result.action = DdgiFilterHistoryAction::Replace;
    result.retention = 0.0;
    bool samePlacement = ddgiAggregatePlacementMatches(current, source);
    float3 nearestEdit = clamp(current.actual_position_and_clearance.xyz, editMin, editMax);
    bool outsideClearance = distance(nearestEdit, current.actual_position_and_clearance.xyz) >
        current.actual_position_and_clearance.w;
    if (samePlacement && outsideClearance)
    {
        result.action = DdgiFilterHistoryAction::Blend;
        result.retention = saturate(retention);
    }
    return result;
}
