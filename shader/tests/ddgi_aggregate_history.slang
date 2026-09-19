import ddgi_filter_policy;
import ddgi_filter_execution;
import ddgi_types;

export __extern_cpp int main()
{
    DdgiProbeMetadata probe;
    probe.nominal_position_and_min_clearance = float4(0, 0, 0, 0.1);
    probe.actual_position_and_clearance = float4(0, 0, 0, 0.5);
    probe.state_and_reserved = uint4(DDGI_PROBE_STATE_VALID, 2, 0, 0);
    DdgiProbeMetadata source = probe;
    source.state_and_reserved.y = 1u;
    float3 editMin = float3(1);
    float3 editMax = float3(2);
    DdgiFilterHistoryDecision stable = ddgiAggregateHistoryDecision(probe, source, editMin, editMax, 0.93);
    if (stable.action != DdgiFilterHistoryAction::Blend || abs(stable.retention - 0.93) > 1e-6) return 1;
    source.actual_position_and_clearance.x += 0.01;
    if (ddgiAggregateHistoryDecision(probe, source, editMin, editMax, 0.93).action != DdgiFilterHistoryAction::Replace) return 2;
    source = probe;
    source.actual_position_and_clearance.w += 0.01;
    if (ddgiAggregateHistoryDecision(probe, source, editMin, editMax, 0.93).action != DdgiFilterHistoryAction::Replace) return 3;
    source = probe;
    source.state_and_reserved.x = DDGI_PROBE_STATE_RELOCATION_FAILED;
    if (ddgiAggregateHistoryDecision(probe, source, editMin, editMax, 0.93).action != DdgiFilterHistoryAction::Replace) return 4;
    if (ddgiAggregateHistoryDecision(probe, probe, float3(-1), float3(1), 0.93).action != DdgiFilterHistoryAction::Replace) return 5;
    // Changed indirect radiance is NOT considered invariant by unchanged placement: every new
    // estimate contributes. Opening and closing both erase 90% of prior error in <=32 updates.
    float4 opening = float4(0, 0, 0, 1);
    float4 closing = float4(1);
    for (uint i = 0; i < 32; ++i) {
        opening = ddgiExecuteHistory(stable, true, float4(1), opening).value;
        closing = ddgiExecuteHistory(stable, true, float4(0, 0, 0, 1), closing).value;
    }
    if (opening.x < 0.9 || closing.x > 0.1) return 6;
    return 0;
}
