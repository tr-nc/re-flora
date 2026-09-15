module leaf_flutter;
import wind_response_curve;

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
public float3 leafWindOffset(float3 globalOffset, float3 localOffset, float globalScale)
{
    // Independent leaf-only translation; never scale hinge motion or optical orientation.
    return globalOffset * max(globalScale, 0.0) + localOffset;
}

public float3 leafLocalOffset(uint seed, float angle)
{
    float3 hinge = leafHinge(seed);
    float3 lever = cross(hinge, leafRestNormal(seed)) * 0.5;
    return leafRotate(lever, hinge, angle) - lever;
}

// Normalize the original half-voxel hinge excursion at the 0.75-radian
// equilibrium limit. Clamp transient overshoot so amplitude is a true radius.
// The reference amplitude preserves the original displacement below that limit.
public float3 leafLocalDisplacement(uint seed, float angle, float amplitudeVoxels)
{
    float3 offset = leafLocalOffset(seed, angle);
    return offset / max(0.36627253, length(offset)) * clamp(amplitudeVoxels, 0.0, 5.0);
}

// Smooth, deterministic local forcing, not a displacement or a second world wind
// field. In particular, constant mean flow must not imply a static leaf pose.
float leafFlutterNoise(float fraction, uint cell, uint seed)
{
    float blend = fraction;
    blend = blend * blend * (3.0 - 2.0 * blend);
    float a = leafUnit(seed ^ leafHash(cell));
    float b = leafUnit(seed ^ leafHash(cell + 1u));
    return lerp(a, b, blend) * 2.0 - 1.0;
}

// Stylized wind-powered petiole excitation. This is not a calibrated botanical
// instability model: zero-mean local forcing is intentionally distinct from the
// overall branch/leaf displacement already handled by advanceLeafResponse.
public float leafTorqueTarget(float2 wind, float2 phase, uint2 cells, uint seed, float gain,
                             float4 curve = float4(0.05, 1.0, 0.0, 0.0))
{
    float speed2 = dot(wind, wind);
    float response = speed2 > 0.0 ? windResponseCurve(sqrt(speed2), curve.x, curve.y, curve.z) : 0.0;
    float3 flow = float3(wind.x, 0.0, wind.y) / max(sqrt(speed2), 0.00001);
    float coupling = lerp(0.65, 1.0, abs(dot(flow, leafRestNormal(seed))));
    float forcing = 0.7 * leafFlutterNoise(phase.x, cells.x, seed)
        + 0.3 * leafFlutterNoise(phase.y, cells.y, seed ^ 0xAD90777Du);
    // Gain changes the envelope, never the wind threshold at which it saturates.
    float envelope = speed2 > 0.0 ? lerp(clamp(curve.w, 0.0, 2.0), clamp(gain, 0.0, 2.0), response) * 0.5 : 0.0;
    return 0.75 * envelope * coupling * forcing;
}

public float leafFlutterFrequency(float2 wind, float4 frequency, float4 curve)
{
    float response = windResponseCurve(length(wind), curve.x, curve.y, curve.z);
    return lerp(clamp(frequency.x, 0.25, 24.0), clamp(frequency.y, 0.25, 24.0), response)
        * clamp(frequency.z, 0.25, 2.0);
}

// Fractional coordinates never grow with world time; integer cells retain the
// seeded noise sequence even after hours of play. Unsigned wrap is continuous.
public void advanceFlutterClock(inout float2 phase, inout uint2 cells, float2 delta)
{
    float2 total = phase + delta;
    uint2 crossed = uint2(floor(total));
    phase = total - float2(crossed);
    cells += crossed;
}

public float2 leafFlutterRates(uint seed, float frequency)
{
    float rate = lerp(2.8, 4.3, leafUnit(seed ^ 0xA341316Cu)) * frequency / 1.8;
    return rate * float2(1.0, 2.37);
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
