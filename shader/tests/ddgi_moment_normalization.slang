import ddgi_query;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    float fullWeight = ddgiMomentNormalizationWeight(0.5, 0.5, 0.2, 4u);
    float broadPartialWeight = ddgiMomentNormalizationWeight(
        0.5, 0.25, 0.15, 3u);
    float isolatedWeight = ddgiMomentNormalizationWeight(
        0.108794592, 0.000287628, 0.00028, 2u);
    float singleProbeWeight = ddgiMomentNormalizationWeight(
        0.5, 0.25, 0.25, 1u);
    float conservativeBoundaryWeight = ddgiMomentNormalizationWeight(
        1.0, 0.5, 0.4921875, 2u);
    float broadBoundaryWeight = ddgiMomentNormalizationWeight(
        1.0, 0.5, 0.4375, 2u);

    printf("full_weight=%.9f broad_partial_weight=%.9f isolated_weight=%.9f "
           "single_probe_weight=%.9f conservative_boundary=%.9f "
           "broad_boundary=%.9f\n",
           fullWeight, broadPartialWeight, isolatedWeight, singleProbeWeight,
           conservativeBoundaryWeight, broadBoundaryWeight);

    bool fullSupportUsesVisibilityWeight = nearlyEqual(fullWeight, 0.5);
    bool broadSupportUsesVisibilityWeight = nearlyEqual(broadPartialWeight, 0.25);
    bool isolatedSupportCannotAmplifyVisibility =
        isolatedWeight > 0.1085 && isolatedWeight < 0.10852;
    bool oneProbeCannotClaimBroadSupport = nearlyEqual(singleProbeWeight, 0.375);
    bool secondarySupportBoundariesAreStable =
        nearlyEqual(conservativeBoundaryWeight, 0.75) &&
        nearlyEqual(broadBoundaryWeight, 0.5);
    return fullSupportUsesVisibilityWeight &&
               broadSupportUsesVisibilityWeight &&
               isolatedSupportCannotAmplifyVisibility &&
               oneProbeCannotClaimBroadSupport &&
               secondarySupportBoundariesAreStable
        ? 0 : 1;
}
