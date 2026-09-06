module wind_field;
import gradient_noise;

// PROTOTYPE transport: no GUI/tool/terrain knowledge; a finite field snapshot.
public struct U_WindFieldInfo {
    public float4 background;
    public float4 transport;
    public float4 detail;
    public float4 gust_origins[16];
    public float4 gust_directions[16];
    public float4 gust_shapes[16];
};
[[vk::binding(4, 0)]] ConstantBuffer<U_WindFieldInfo> wind_field_info;

public bool authoredWindEnabled() { return wind_field_info.background.w >= 1.0; }

float fieldNoise(float2 p, uint seed) {
    // Keep the existing smooth, multiscale gradient-noise building block. Fixed
    // normalized weights make detail scale independent of average wind strength.
    return 0.6 * seededClassicGradientNoise2D(p, seed)
         + 0.3 * seededClassicGradientNoise2D(p * 2.0, seed + 1000u)
         + 0.1 * seededClassicGradientNoise2D(p * 4.0, seed + 2000u);
}

public float2 sampleAuthoredWind(float2 worldVoxels)
{
    float2 direction = wind_field_info.background.xy;
    float time = wind_field_info.transport.z;
    float2 p = (worldVoxels - wind_field_info.transport.xy) / max(wind_field_info.detail.x, 1.0);
    float along = fieldNoise(p, 3181u);
    float2 result = direction * wind_field_info.background.z * max(0.0, 1.0 + along * 0.25);
    if (wind_field_info.background.w >= 2.0) {
        // Smooth time evolution in addition to advection. This is an art-directed
        // vector disturbance, not a divergence-free fluid or terrain-flow solve.
        float phase = time * wind_field_info.detail.z;
        float2 evolution = float2(sin(phase * 0.7), cos(phase * 0.43)) * 1.7;
        float side = fieldNoise(p * 1.3 + evolution, 7919u);
        float detail = fieldNoise(p * 2.7 - evolution * 0.5, 1543u);
        result += (float2(-direction.y, direction.x) * side + direction * detail * 0.4) * wind_field_info.detail.y;
    }
    return result;
}

// Local events are independent of the selected background wind model.
public float2 sampleWindGusts(float2 worldVoxels)
{
    float2 result = float2(0.0);
    float time = wind_field_info.transport.z;
    uint count = min(uint(wind_field_info.detail.w), 16u);
    for (uint i = 0u; i < count; ++i) {
        float4 origin = wind_field_info.gust_origins[i];
        float4 motion = wind_field_info.gust_directions[i];
        float4 shape = wind_field_info.gust_shapes[i];
        float age = time - origin.z;
        if (age < 0.0 || age >= shape.z) continue;
        float life = age / max(shape.z, 0.001);
        float envelope = smoothstep(0.0, 0.15, life) * (1.0 - smoothstep(0.65, 1.0, life));
        float2 delta = worldVoxels - origin.xy;
        float2 push;
        float spatial;
        float innerEdge = 1.0 - clamp(shape.w, 0.05, 1.0);
        if (origin.w >= 0.5) {
            float radius = length(delta);
            push = radius > 0.0001 ? delta / radius : float2(0.0);
            float distance = abs(radius - age * motion.w) / max(shape.x, 1.0);
            spatial = 1.0 - smoothstep(innerEdge, 1.0, distance);
        } else {
            push = motion.xy;
            float2 local = delta - push * age * motion.w;
            float forward = dot(local, push) / max(shape.x, 1.0);
            float side = dot(local, float2(-push.y, push.x)) / max(shape.y, 1.0);
            // A crosswind band: strongest at its center, fading smoothly to
            // both side edges and to the front/back. Zero slope at the bounds
            // prevents a hard force edge as the band sweeps over a plant.
            spatial = (1.0 - smoothstep(innerEdge, 1.0, abs(forward)))
                    * (1.0 - smoothstep(innerEdge, 1.0, abs(side)));
        }
        result += push * motion.z * envelope * spatial;
    }
    return result;
}
