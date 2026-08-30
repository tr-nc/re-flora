module ddgi_irradiance_filter_owner;

import ddgi_filter_evidence;
import ddgi_filter_execution;
import ddgi_filter_policy;

[[vk::binding(27, 0)]]
RWTexture2D<float4> ddgi_irradiance_atlas;

[[vk::binding(29, 0)]]
RWTexture2D<float4> ddgi_transport_source_irradiance_atlas;

public float4 ddgiOwnerLoadIrradiance(uint slot, uint2 coordinate)
{
    return slot == 0u
        ? ddgi_irradiance_atlas[coordinate]
        : ddgi_transport_source_irradiance_atlas[coordinate];
}

void ddgiOwnerStoreIrradianceValue(
    uint destinationIsTransportSource, uint2 coordinate, float4 value)
{
    if (destinationIsTransportSource != 0u)
        ddgi_transport_source_irradiance_atlas[coordinate] = value;
    else
        ddgi_irradiance_atlas[coordinate] = value;
}

public bool ddgiOwnerTryStoreTerminalIrradiance(
    uint evidenceEnabled, bool electedProbeLane,
    uint destinationIsTransportSource, uint2 coordinate,
    DdgiFilterHistoryDecision requested, bool freshAvailable, float4 source)
{
    if (requested.action != DdgiFilterHistoryAction::Retain && freshAvailable)
        return false;
    DdgiExecutedHistory4 executed = ddgiExecuteHistory(
        requested, freshAvailable, float4(0.0), source);
    ddgiRecordIrradianceHistoryDecision(
        evidenceEnabled, electedProbeLane, executed.decision);
    ddgiOwnerStoreIrradianceValue(
        destinationIsTransportSource, coordinate, executed.value);
    return true;
}

public void ddgiOwnerStoreFreshIrradiance(
    uint evidenceEnabled, bool electedProbeLane,
    uint destinationIsTransportSource, uint2 coordinate,
    DdgiFilterHistoryDecision requested, float4 fresh, float4 source)
{
    DdgiExecutedHistory4 executed = ddgiExecuteHistory(
        requested, true, fresh, source);
    ddgiRecordIrradianceHistoryDecision(
        evidenceEnabled, electedProbeLane, executed.decision);
    ddgiOwnerStoreIrradianceValue(
        destinationIsTransportSource, coordinate, executed.value);
}
