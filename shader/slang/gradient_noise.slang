module gradient_noise;

float4 mod289(float4 value)
{
    return value - floor(value * (1.0 / 289.0)) * 289.0;
}

float4 permute(float4 value)
{
    return mod289(((value * 34.0) + 1.0) * value);
}

float4 taylorInverseSqrt(float4 value)
{
    return 1.79284291400159 - 0.85373472095314 * value;
}

float2 quintic(float2 value)
{
    return value * value * value *
           (value * (value * 6.0 - 15.0) + 10.0);
}

public float classicGradientNoise2D(float2 point)
{
    float4 integerPart = floor(point.xyxy) + float4(0.0, 0.0, 1.0, 1.0);
    float4 fractionalPart = frac(point.xyxy) - float4(0.0, 0.0, 1.0, 1.0);
    integerPart = mod289(integerPart);
    float4 ix = integerPart.xzxz;
    float4 iy = integerPart.yyww;
    float4 fx = fractionalPart.xzxz;
    float4 fy = fractionalPart.yyww;

    float4 permutation = permute(permute(ix) + iy);
    float4 gx = frac(permutation * (1.0 / 41.0)) * 2.0 - 1.0;
    float4 gy = abs(gx) - 0.5;
    float4 tx = floor(gx + 0.5);
    gx -= tx;

    float2 g00 = float2(gx.x, gy.x);
    float2 g10 = float2(gx.y, gy.y);
    float2 g01 = float2(gx.z, gy.z);
    float2 g11 = float2(gx.w, gy.w);

    float4 normalization = taylorInverseSqrt(float4(
        dot(g00, g00), dot(g01, g01), dot(g10, g10), dot(g11, g11)));
    g00 *= normalization.x;
    g01 *= normalization.y;
    g10 *= normalization.z;
    g11 *= normalization.w;

    float n00 = dot(g00, float2(fx.x, fy.x));
    float n10 = dot(g10, float2(fx.y, fy.y));
    float n01 = dot(g01, float2(fx.z, fy.z));
    float n11 = dot(g11, float2(fx.w, fy.w));

    float2 fade = quintic(fractionalPart.xy);
    float2 nx = lerp(float2(n00, n01), float2(n10, n11), fade.x);
    return 2.3 * lerp(nx.x, nx.y, fade.y);
}

public float seededClassicGradientNoise2D(float2 point, uint seed)
{
    float sx = float((seed * 73856093u) & 0xffffu) * 0.153;
    float sy = float((seed * 19349663u) & 0xffffu) * 0.217;
    return classicGradientNoise2D(point + float2(sx, sy));
}

public float fbmClassicGradientNoise2D(float x,
                                       float y,
                                       uint seed,
                                       float frequency,
                                       int octaves,
                                       float lacunarity,
                                       float gain)
{
    float sum = 0.0;
    float amplitude = 1.0;
    float amplitudeSum = 0.0;
    float currentFrequency = frequency;
    for (int i = 0; i < octaves; ++i)
    {
        sum += seededClassicGradientNoise2D(
            float2(x, y) * currentFrequency,
            seed + uint(i) * 1000u) * amplitude;
        amplitudeSum += amplitude;
        currentFrequency *= lacunarity;
        amplitude *= gain;
    }
    return sum / amplitudeSum;
}
