module leaf_flutter;

// A cube represents an unresolved thin leaf. This frame is optical/mechanical
// state only: never apply it to cube vertices, face normals, or ray origins.
uint leafHash(uint x)
{
    x = x * 747796405u + 2891336453u;
    uint word = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (word >> 22u) ^ word;
}
float leafUnit(uint seed) { return float(leafHash(seed) & 65535u) / 65535.0; }

public float3 leafRestNormal(uint seed)
{
    float azimuth = leafUnit(seed) * 6.28318530718;
    float elevation = lerp(0.2, 1.15, leafUnit(seed ^ 0xB5297A4Du));
    return float3(cos(azimuth) * cos(elevation), sin(elevation), sin(azimuth) * cos(elevation));
}
public float3 leafHinge(uint seed)
{
    float azimuth = leafUnit(seed) * 6.28318530718;
    return float3(-sin(azimuth), 0.0, cos(azimuth));
}
float3 leafRotate(float3 v, float3 axis, float angle)
{
    return v * cos(angle) + cross(axis, v) * sin(angle) + axis * dot(axis, v) * (1.0 - cos(angle));
}
public float3 leafOpticalNormal(uint seed, float angle)
{
    return leafRotate(leafRestNormal(seed), leafHinge(seed), angle);
}
public float3 leafLocalOffset(uint seed, float angle)
{
    float3 hinge = leafHinge(seed);
    float3 lever = cross(hinge, leafRestNormal(seed)) * 0.5;
    return leafRotate(lever, hinge, angle) - lever;
}

// Pressure projected onto a proxy leaf plus continuous, unresolved turbulence.
// Wind is in game units; the bounded equilibrium angle is an art calibration.
public float leafTorqueTarget(float2 wind, float angle, float time, uint seed, float gain)
{
    float speed2 = dot(wind, wind);
    // Reference speed 0.3 in the game field makes a breeze excite local motion.
    float pressure = speed2 / (0.09 + speed2);
    float3 flow = float3(wind.x, 0.0, wind.y) / max(sqrt(speed2), 0.00001);
    float3 normal = leafOpticalNormal(seed, angle);
    float3 lever = cross(leafHinge(seed), normal);
    float projected = abs(dot(flow, normal));
    float torque = dot(cross(lever, flow), leafHinge(seed)) * projected;
    float phase = leafUnit(seed ^ 0x68E31DA4u) * 6.28318530718;
    float frequency = lerp(0.9, 1.5, leafUnit(seed ^ 0x1B56C4E9u));
    float turbulence = sin(time * frequency * 6.28318530718 + phase)
        + 0.35 * sin(time * frequency * 0.61 * 6.28318530718 + phase * 1.7);
    float target = max(gain, 0.0) * pressure * (0.35 * torque + 0.65 * turbulence);
    return 0.75 * target / (1.0 + abs(target));
}

// One RGB evaluated at the leaf center, shared by every corner and every face.
// Irradiance remains the existing direction-averaged/stable canopy cache.
public float3 leafOpticalColor(float3 baseColor, float3 environmentLight,
    float3 sunRadiance, float visibility, float3 view, float3 light,
    float3 normal, float transmissionStrength)
{
    float front = smoothstep(-0.18, 0.18, dot(normal, view));
    float3 backColor = lerp(baseColor, sqrt(max(baseColor, float3(0.0))), 0.28);
    float3 sideColor = lerp(backColor, baseColor, front);
    float incidence = dot(normal, light);
    float reflected = lerp(max(-incidence, 0.0), max(incidence, 0.0), front);
    float transmitted = lerp(max(incidence, 0.0), max(-incidence, 0.0), front);
    float through = 0.35 * clamp(transmissionStrength, 0.0, 1.0);
    float3 scatter = sideColor * ((1.0 - through) * reflected)
        + sqrt(max(baseColor, float3(0.0))) * (through * transmitted);
    return sideColor * environmentLight + scatter * sunRadiance * clamp(visibility, 0.0, 1.0);
}
