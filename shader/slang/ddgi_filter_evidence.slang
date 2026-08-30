module ddgi_filter_evidence;

import ddgi_filter_policy;
import ddgi_types;

[[vk::binding(26, 0)]]
GLSLShaderStorageBuffer<B_DdgiTraceStats, Std430DataLayout>
    ddgi_trace_stats;

static const uint DDGI_IRRADIANCE_HISTORY_OWNER_SUM = 13u;
static const uint DDGI_IRRADIANCE_HISTORY_PROBES = 14u;
static const uint DDGI_IRRADIANCE_HISTORY_REPLACE = 15u;
static const uint DDGI_IRRADIANCE_HISTORY_RETAIN = 16u;
static const uint DDGI_IRRADIANCE_HISTORY_BLEND = 17u;
static const uint DDGI_IRRADIANCE_HISTORY_RETENTION_Q16 = 18u;
static const uint DDGI_VISIBILITY_HISTORY_OWNER_SUM = 19u;
static const uint DDGI_VISIBILITY_HISTORY_PROBES = 20u;
static const uint DDGI_VISIBILITY_HISTORY_REPLACE = 21u;
static const uint DDGI_VISIBILITY_HISTORY_RETAIN = 22u;
static const uint DDGI_VISIBILITY_HISTORY_BLEND = 23u;
static const uint DDGI_VISIBILITY_HISTORY_RETENTION_Q16 = 24u;
static const uint DDGI_VISIBILITY_SAMPLE_OWNER_SUM = 25u;
static const uint DDGI_VISIBILITY_SAMPLE_COUNT = 26u;
static const uint DDGI_VISIBILITY_SAMPLE_ACCEPT = 27u;
static const uint DDGI_VISIBILITY_SAMPLE_REJECT = 28u;

void ddgiRecordFilterStat(uint lane, uint value)
{
    if (value != 0u) InterlockedAdd(ddgi_trace_stats.data[lane], value);
}

uint ddgiFilterRetentionQ16(float retention)
{
    return uint(round(saturate(retention) * 65536.0));
}

void ddgiRecordHistoryDecision(
    uint evidenceEnabled, bool electedProbeLane,
    DdgiFilterHistoryDecision decision, uint ownerLane, uint probesLane,
    uint replaceLane, uint retainLane, uint blendLane, uint retentionLane)
{
    if (evidenceEnabled == 0u || !electedProbeLane) return;
    ddgiRecordFilterStat(ownerLane, decision.owner_version);
    ddgiRecordFilterStat(probesLane, 1u);
    switch (decision.action)
    {
    case DdgiFilterHistoryAction::Retain:
        ddgiRecordFilterStat(retainLane, 1u);
        break;
    case DdgiFilterHistoryAction::Blend:
        ddgiRecordFilterStat(blendLane, 1u);
        ddgiRecordFilterStat(
            retentionLane, ddgiFilterRetentionQ16(decision.retention));
        break;
    default:
        ddgiRecordFilterStat(replaceLane, 1u);
        break;
    }
}

public void ddgiRecordIrradianceHistoryDecision(
    uint evidenceEnabled, bool electedProbeLane,
    DdgiFilterHistoryDecision decision)
{
    ddgiRecordHistoryDecision(
        evidenceEnabled, electedProbeLane, decision,
        DDGI_IRRADIANCE_HISTORY_OWNER_SUM,
        DDGI_IRRADIANCE_HISTORY_PROBES,
        DDGI_IRRADIANCE_HISTORY_REPLACE,
        DDGI_IRRADIANCE_HISTORY_RETAIN,
        DDGI_IRRADIANCE_HISTORY_BLEND,
        DDGI_IRRADIANCE_HISTORY_RETENTION_Q16);
}

public void ddgiRecordVisibilityHistoryDecision(
    uint evidenceEnabled, bool electedProbeLane,
    DdgiFilterHistoryDecision decision)
{
    ddgiRecordHistoryDecision(
        evidenceEnabled, electedProbeLane, decision,
        DDGI_VISIBILITY_HISTORY_OWNER_SUM,
        DDGI_VISIBILITY_HISTORY_PROBES,
        DDGI_VISIBILITY_HISTORY_REPLACE,
        DDGI_VISIBILITY_HISTORY_RETAIN,
        DDGI_VISIBILITY_HISTORY_BLEND,
        DDGI_VISIBILITY_HISTORY_RETENTION_Q16);
}

public void ddgiRecordVisibilitySampleDecision(
    uint evidenceEnabled, bool electedEvidenceLane,
    DdgiFilterVisibilityDecision decision)
{
    if (evidenceEnabled == 0u || !electedEvidenceLane) return;
    ddgiRecordFilterStat(
        DDGI_VISIBILITY_SAMPLE_OWNER_SUM, decision.owner_version);
    ddgiRecordFilterStat(DDGI_VISIBILITY_SAMPLE_COUNT, 1u);
    ddgiRecordFilterStat(
        decision.action == DdgiFilterVisibilityAction::Accept
            ? DDGI_VISIBILITY_SAMPLE_ACCEPT
            : DDGI_VISIBILITY_SAMPLE_REJECT,
        1u);
}
