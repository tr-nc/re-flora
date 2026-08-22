import ddgi_query;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    float2 moments = float2(1.0, 2.0);
    float unoccluded = ddgiChebyshevVisibility(moments, 1.0);
    float visible = ddgiChebyshevVisibility(moments, 2.0);
    float marginalFalsePositive = ddgiChebyshevVisibility(
        moments, 2.05);
    float lowConfidence = ddgiChebyshevVisibility(
        moments, 2.1);

    printf("unoccluded=%.9f visible=%.9f marginal=%.9f low_confidence=%.9f\n",
           unoccluded, visible, marginalFalsePositive, lowConfidence);

    bool stableValues = nearlyEqual(unoccluded, 1.0) &&
        nearlyEqual(visible, 0.125);
    bool lowConfidenceFailsClosed =
        nearlyEqual(marginalFalsePositive, 0.0) &&
        nearlyEqual(lowConfidence, 0.0);
    return stableValues && lowConfidenceFailsClosed ? 0 : 1;
}
