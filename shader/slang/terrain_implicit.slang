module terrain_implicit;

public float smoothMinimumQuadratic(float a, float b, float maximumInflation)
{
    if (maximumInflation <= 0.0)
    {
        return min(a, b);
    }

    float differenceSupport = maximumInflation * 4.0;
    float h = max(differenceSupport - abs(a - b), 0.0) /
              differenceSupport;
    return min(a, b) - h * h * differenceSupport * 0.25;
}

public float smoothMaximumQuadratic(float a, float b, float maximumInflation)
{
    return -smoothMinimumQuadratic(-a, -b, maximumInflation);
}
