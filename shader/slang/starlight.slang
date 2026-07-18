module starlight;

import color;

public struct StarlightInfo
{
    public int iterations;
    public float formuparam;
    public int volsteps;
    public float stepsize;
    public float zoom;
    public float tile;
    public float speed;
    public float brightness;
    public float darkmatter;
    public float distfading;
    public float saturation;
};

float3 positiveMod(float3 value, float divisor)
{
    return value - divisor * floor(value / divisor);
}

float3 starNestEffect(float3 viewDirection, StarlightInfo info)
{
    float3 direction = viewDirection;
    float3 from = float3(1.0, 0.5, 2.0);
    float distance = 0.1;
    float fade = 1.0;
    float3 color = float3(0.0);

    for (int volumeStep = 0; volumeStep < info.volsteps; volumeStep++)
    {
        float3 position = from + distance * direction * 0.5;
        position = abs(float3(info.tile) - positiveMod(position, info.tile * 2.0));

        float previousLength = 0.0;
        float accumulation = 0.0;
        for (int iteration = 0; iteration < info.iterations; iteration++)
        {
            position = abs(position) / (dot(position, position) + 1.0e-6) - info.formuparam;
            float positionLength = length(position);
            accumulation += min(abs(positionLength - previousLength), 10.0);
            previousLength = positionLength;
        }

        float darkMatter = max(0.0, info.darkmatter - accumulation * accumulation * 0.001);
        accumulation *= accumulation * accumulation;
        if (volumeStep > 6) fade *= 1.0 - darkMatter;

        color += fade;
        color += float3(distance, distance * distance,
                        distance * distance * distance * distance) *
                 accumulation * info.brightness * fade;
        fade *= info.distfading;
        distance += info.stepsize;
    }

    color = lerp(float3(length(color)), color, info.saturation);
    return color * 0.01;
}

public float3 getStarlightColor(float3 viewDirection, StarlightInfo info)
{
    return srgbToLinear(starNestEffect(viewDirection, info));
}
