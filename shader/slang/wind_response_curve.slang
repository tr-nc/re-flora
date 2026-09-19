module wind_response_curve;

public float windResponseCurve(float speed, float start, float full, float knee)
{
    float low = min(start, full);
    float high = max(start, full);
    float x = high - low <= 1.0e-4 ? (speed >= high ? 1.0 : 0.0)
        : clamp((speed - low) / (high - low), 0.0, 1.0);
    x = pow(x, exp2(clamp(knee, -6.0, 6.0)));
    return x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
}
