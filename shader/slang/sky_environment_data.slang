module sky_environment_data;

public static const uint SKY_COLOR_KEYFRAME_COUNT = 11u;

public static const float SKY_COLOR_ALTITUDES[SKY_COLOR_KEYFRAME_COUNT] =
{
    -0.40, -0.25, -0.15, -0.07, -0.03, 0.0, 0.1, 0.3, 0.5, 0.8, 1.0
};

public static const float3 SKY_COLOR_TOP[SKY_COLOR_KEYFRAME_COUNT] =
{
    float3(0.001214, 0.001518, 0.003035),
    float3(0.006049, 0.008023, 0.029557),
    float3(0.005182, 0.010330, 0.061246),
    float3(0.008023, 0.023153, 0.127438),
    float3(0.011612, 0.029557, 0.141263),
    float3(0.061246, 0.114435, 0.428690),
    float3(0.187821, 0.341914, 0.938686),
    float3(0.042311, 0.223228, 0.715694),
    float3(0.019382, 0.149960, 0.644480),
    float3(0.011612, 0.107023, 0.545724),
    float3(0.023153, 0.141263, 0.644480)
};

public static const float3 SKY_COLOR_BOTTOM[SKY_COLOR_KEYFRAME_COUNT] =
{
    float3(0.002428, 0.003035, 0.006995),
    float3(0.038204, 0.019382, 0.042311),
    float3(0.014444, 0.011612, 0.064803),
    float3(0.031896, 0.034340, 0.155926),
    float3(0.545724, 0.045186, 0.090842),
    float3(1.000000, 0.171441, 0.026241),
    float3(1.000000, 0.485150, 0.114435),
    float3(0.127438, 0.351533, 0.715694),
    float3(0.090842, 0.283149, 0.679542),
    float3(0.064803, 0.230740, 0.610496),
    float3(0.097587, 0.296138, 0.679542)
};
