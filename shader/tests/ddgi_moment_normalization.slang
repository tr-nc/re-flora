import ddgi_query;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    float normalWeight = ddgiMomentNormalizationWeight(0.5, 0.125);
    float bd2Weight = ddgiMomentNormalizationWeight(
        0.108794592, 0.000287628);

    printf("normal_weight=%.9f bd2_weight=%.9f\n",
           normalWeight, bd2Weight);

    bool normalSupportIsUnchanged = nearlyEqual(normalWeight, 0.125);
    bool isolatedLowSupportUsesConfidenceFloor =
        nearlyEqual(bd2Weight, 0.013599324);
    return normalSupportIsUnchanged && isolatedLowSupportUsesConfidenceFloor
        ? 0 : 1;
}
