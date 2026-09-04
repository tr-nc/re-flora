import ddgi_query;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-6;
}

export __extern_cpp int main()
{
    bool biasClamp = nearlyEqual(ddgiVisibilityBiasWorld(-0.25), 0.0) &&
        nearlyEqual(ddgiVisibilityBiasWorld(0.125), 0.125);
    bool domainPrecedence =
        ddgiQueryDomain(false, false) == DDGI_QUERY_DOMAIN_LOCAL &&
        ddgiQueryDomain(false, true) == DDGI_QUERY_DOMAIN_GLOBAL_SKY &&
        ddgiQueryDomain(true, false) == DDGI_QUERY_DOMAIN_INVALIDATED &&
        ddgiQueryDomain(true, true) == DDGI_QUERY_DOMAIN_INVALIDATED;
    bool visibilityModes = nearlyEqual(
            ddgiQueryVisibilityWeight(0.6, 0.25, DDGI_VISIBILITY_MOMENT), 0.6) &&
        nearlyEqual(ddgiQueryVisibilityWeight(
            0.6, 0.25, DDGI_VISIBILITY_MOMENT_EXACT), 0.15) &&
        nearlyEqual(ddgiQueryVisibilityWeight(
            0.6, 0.25, DDGI_VISIBILITY_EXACT), 0.25) &&
        nearlyEqual(ddgiQueryVisibilityWeight(0.6, 0.25, 99u), 0.0);
    return biasClamp && domainPrecedence && visibilityModes ? 0 : 1;
}
