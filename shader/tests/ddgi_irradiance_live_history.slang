import ddgi_filter_policy;
import ddgi_filter_execution;

export __extern_cpp int main()
{
    // A geometric dirty bound cannot prove unchanged incident radiance outside it:
    // light mutations and changed remote bounce paths must contribute every sweep.
    DdgiFilterHistoryDecision remote = ddgiFilterIrradianceHistoryDecision(
        1u, 1u, 100u, 0.93);
    if (remote.action != DdgiFilterHistoryAction::Blend) return 1;
    if (abs(remote.retention - 0.93) > 1.0e-6) return 2;
    float4 dark = float4(0.0, 0.0, 0.0, 1.0);
    float4 lit = float4(1.0);
    float4 rising = ddgiExecuteHistory(remote, true, lit, dark).value;
    float4 falling = ddgiExecuteHistory(remote, true, dark, lit).value;
    if (!all(rising.xyz > dark.xyz) || !all(rising.xyz < lit.xyz)) return 3;
    if (!all(falling.xyz < lit.xyz) || !all(falling.xyz > dark.xyz)) return 4;

    // The visibility-moment policy is independent and still preserves the remote partition.
    DdgiFilterHistoryDecision visibility = ddgiFilterHistoryDecision(
        1u, 1u, false, 0u, 0.93);
    if (visibility.action != DdgiFilterHistoryAction::Retain) return 5;
    if (!all(ddgiExecuteHistory(visibility, true, lit, dark).value == dark)) return 6;

    DdgiFilterHistoryDecision firstRecovery = ddgiFilterIrradianceHistoryDecision(
        1u, 1u, 0u, 0.93);
    if (!all(ddgiExecuteHistory(firstRecovery, true, lit, dark).value == lit)) return 7;
    DdgiFilterHistoryDecision noHistory = ddgiFilterIrradianceHistoryDecision(
        0u, 1u, 0u, 0.93);
    if (noHistory.action != DdgiFilterHistoryAction::Replace) return 8;
    if (!all(ddgiExecuteHistory(noHistory, true, lit, dark).value == lit)) return 9;

    DdgiFilterHistoryDecision bounded = ddgiFilterIrradianceHistoryDecision(
        1u, 1u, 100u, 1.0);
    if (bounded.retention >= 1.0) return 10;
    if (!all(ddgiExecuteHistory(bounded, true, lit, dark).value.xyz > dark.xyz)) return 11;
    if (!all(ddgiExecuteHistory(remote, false, lit, lit).value == float4(0.0))) return 12;
    DdgiFilterHistoryDecision recovering = ddgiFilterIrradianceHistoryDecision(
        1u, 1u, 2u, 0.93);
    if (abs(recovering.retention - 2.0 / 3.0) > 1.0e-6) return 13;
    return 0;
}
