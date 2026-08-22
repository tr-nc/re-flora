import ddgi_query;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    float fullWeight = ddgiMomentNormalizationWeight(0.5, 0.5);
    float partialWeight = ddgiMomentNormalizationWeight(0.5, 0.25);
    float sparseWeight = ddgiMomentNormalizationWeight(
        0.108794592, 0.000287628);

    printf("full_weight=%.9f partial_weight=%.9f sparse_weight=%.9f\n",
           fullWeight, partialWeight, sparseWeight);

    bool fullSupportUsesVisibilityWeight = nearlyEqual(fullWeight, 0.5);
    bool partialSupportBlendsContinuously = nearlyEqual(partialWeight, 0.375);
    bool isolatedSupportCannotAmplifyVisibility =
        nearlyEqual(sparseWeight, 0.108507724);
    return fullSupportUsesVisibilityWeight &&
               partialSupportBlendsContinuously &&
               isolatedSupportCannotAmplifyVisibility
        ? 0 : 1;
}
