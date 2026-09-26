import ddgi_config;
import ddgi_filter_evidence;
import ddgi_filter_execution;
import ddgi_filter_policy;

// Only the storage adapter differs from Vulkan. Lane selection, gating, counts,
// owner-bit encoding and retention witnesses execute the production producers.
struct Evidence : IDdgiFilterEvidenceSink
{
    uint lanes[31];

    [mutating] void add(uint lane, uint value) { lanes[lane] += value; }
    [mutating] void bitOr(uint lane, uint value) { lanes[lane] |= value; }
    [mutating] void maximum(uint lane, uint value) { lanes[lane] = max(lanes[lane], value); }
}

Evidence emptyEvidence()
{
    Evidence result;
    for (uint i = 0u; i < 31u; ++i) result.lanes[i] = 0u;
    return result;
}

bool isEmpty(Evidence evidence)
{
    for (uint i = 0u; i < 31u; ++i)
        if (evidence.lanes[i] != 0u) return false;
    return true;
}

void recordFreshProbe(inout Evidence evidence, DdgiFilterHistoryDecision requested)
{
    for (uint i = 0u; i < DDGI_RAYS_PER_PROBE; ++i)
    {
        DdgiFilterVisibilityDecision sample = ddgiFilterVisibilityDecision(
            i % 4u == 3u ? 8.0 : 2.0, 4.0, 100.0);
        ddgiRecordVisibilitySampleDecision(evidence, 1u, true, sample);
    }
    DdgiExecutedHistory2 result = ddgiExecuteHistory(requested, true, float2(1.0), float2(2.0));
    ddgiRecordVisibilityFilterResult(evidence, 1u, true, result.decision);
}

export __extern_cpp int main()
{
    DdgiFilterHistoryDecision retain = ddgiFilterHistoryDecision(1u, 1u, false, 0u, 0.9);
    DdgiFilterHistoryDecision blend = ddgiFilterHistoryDecision(1u, 1u, true, 1u, 0.9);
    DdgiFilterHistoryDecision replace = ddgiFilterHistoryDecision(0u, 0u, false, 0u, 0.9);

    // Minimal native failure: the terminal store completes Retain without visiting
    // the sample loop. Its evidence must still identify the visibility policy owner.
    Evidence terminal = emptyEvidence();
    for (uint freshAvailable = 0u; freshAvailable < 2u; ++freshAvailable)
    {
        DdgiExecutedHistory2 result = ddgiExecuteHistory(
            retain, freshAvailable != 0u, float2(0.0), float2(2.0, 4.0));
        if (!all(result.value == float2(2.0, 4.0))) return 1;
        ddgiRecordVisibilityFilterResult(terminal, 1u, true, result.decision);
    }
    if (terminal.lanes[19] != 2u || terminal.lanes[20] != 2u || terminal.lanes[22] != 2u) return 2;
    if (terminal.lanes[25] != 2u) return 3;
    if (terminal.lanes[26] != 0u || terminal.lanes[27] != 0u || terminal.lanes[28] != 0u) return 4;

    // Invalid fresh data terminates as zero Replace, not a Blend with invented rays.
    DdgiExecutedHistory2 invalid = ddgiExecuteHistory(blend, false, float2(1.0), float2(2.0));
    Evidence noFresh = emptyEvidence();
    ddgiRecordVisibilityFilterResult(noFresh, 1u, true, invalid.decision);
    if (!all(invalid.value == float2(0.0)) || noFresh.lanes[21] != 1u || noFresh.lanes[23] != 0u) return 5;
    if (noFresh.lanes[25] != 2u || noFresh.lanes[26] != 0u || noFresh.lanes[24] != 0u || noFresh.lanes[30] != 0u) return 6;

    Evidence fresh = emptyEvidence();
    recordFreshProbe(fresh, replace);
    recordFreshProbe(fresh, blend);
    if (fresh.lanes[20] != 2u || fresh.lanes[21] != 1u || fresh.lanes[22] != 0u || fresh.lanes[23] != 1u) return 7;
    if (fresh.lanes[24] != 32768u || fresh.lanes[30] != 32768u) return 8;
    if (fresh.lanes[25] != 2u || fresh.lanes[26] != 128u || fresh.lanes[27] != 96u || fresh.lanes[28] != 32u) return 9;

    Evidence mixed = terminal;
    ddgiRecordVisibilityFilterResult(mixed, 1u, true, invalid.decision);
    recordFreshProbe(mixed, replace);
    recordFreshProbe(mixed, blend);
    if (mixed.lanes[19] != 2u || mixed.lanes[20] != 5u || mixed.lanes[21] != 2u || mixed.lanes[22] != 2u || mixed.lanes[23] != 1u) return 10;
    if (mixed.lanes[25] != 2u || mixed.lanes[26] != 128u || mixed.lanes[27] != 96u || mixed.lanes[28] != 32u) return 11;
    // The visibility producer must not write irradiance evidence.
    for (uint i = 0u; i < 19u; ++i)
        if (mixed.lanes[i] != 0u) return 12;
    if (mixed.lanes[29] != 0u) return 13;

    DdgiFilterVisibilityDecision sample = ddgiFilterVisibilityDecision(2.0, 4.0, 100.0);
    Evidence disabled = emptyEvidence();
    ddgiRecordVisibilityFilterResult(disabled, 0u, true, retain);
    ddgiRecordVisibilityFilterResult(disabled, 0u, true, invalid.decision);
    ddgiRecordVisibilitySampleDecision(disabled, 0u, true, sample);
    ddgiRecordVisibilityFilterResult(disabled, 0u, true, blend);
    if (!isEmpty(disabled)) return 14;
    ddgiRecordVisibilityFilterResult(disabled, 1u, false, retain);
    ddgiRecordVisibilityFilterResult(disabled, 1u, false, invalid.decision);
    ddgiRecordVisibilitySampleDecision(disabled, 1u, false, sample);
    ddgiRecordVisibilityFilterResult(disabled, 1u, false, blend);
    if (!isEmpty(disabled)) return 15;

    // Completion ORs its owner witness; it cannot overwrite a wrong sample owner.
    Evidence wrongOwner = emptyEvidence();
    sample.owner_version = 2u;
    ddgiRecordVisibilitySampleDecision(wrongOwner, 1u, true, sample);
    ddgiRecordVisibilityFilterResult(wrongOwner, 1u, true, blend);
    if (wrongOwner.lanes[25] != 6u) return 16;
    sample.owner_version = 31u;
    ddgiRecordVisibilitySampleDecision(wrongOwner, 1u, true, sample);
    if (wrongOwner.lanes[25] != 0x80000006u) return 17;
    retain.owner_version = 2u;
    ddgiRecordVisibilityFilterResult(wrongOwner, 1u, true, retain);
    if (wrongOwner.lanes[19] != 6u) return 18;
    return 0;
}
