module terrain_height;

import gradient_noise;

public float computeIslandShape2D(float2 worldXZ)
{
    static const float MAP_SIZE = 5.0;
    float2 uv = worldXZ / MAP_SIZE;
    float2 center = (uv - float2(0.5)) * 2.0;
    float distanceFromCenter = length(center);
    float angle = atan2(center.y, center.x);

    float2 coastSample = float2(cos(angle), sin(angle)) * 2.5 + float2(10.0);
    float coastNoise = fbmClassicGradientNoise2D(
        coastSample.x, coastSample.y, 133u, 0.2, 5, 2.0, 0.5);
    float coastWarp = coastNoise * 0.3;

    float thresholdNoise = fbmClassicGradientNoise2D(
        worldXZ.x, worldXZ.y, 777u, 0.1, 3, 2.0, 0.5);
    float threshold = thresholdNoise * 0.5 + 0.5;
    float coastInner = lerp(0.33, 0.88, threshold);
    return clamp(
        1.0 - smoothstep(
            coastInner, 0.88, distanceFromCenter + coastWarp),
        0.0,
        1.0);
}

public float computeBaseShape2D(float2 worldXZ)
{
    static const float TERRAIN_RELIEF_SCALE = 0.85;
    float base = fbmClassicGradientNoise2D(
        worldXZ.x, worldXZ.y, 42u, 0.15, 5, 2.2, 0.55);
    float detail = fbmClassicGradientNoise2D(
        worldXZ.x, worldXZ.y, 999u, 0.8, 4, 2.0, 0.5);
    float fine = fbmClassicGradientNoise2D(
        worldXZ.x, worldXZ.y, 5555u, 3.0, 3, 2.0, 0.5);
    float relief = base * 0.5 + detail * 0.15 + fine * 0.04;
    return 0.5 + relief * TERRAIN_RELIEF_SCALE;
}

public float computeSurfaceHeight2D(float2 worldXZ)
{
    float baseShape = computeBaseShape2D(worldXZ);
    float islandShape = computeIslandShape2D(worldXZ);
    return baseShape;
}
