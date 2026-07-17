module wind_volume_sample;

import definitions;
import gradient_noise;
import tracer_types;

static const uint WIND_FBM_SEED = 3181u;
static const float WIND_SAMPLE_SCALE = 256.0;
static const float WIND_TIME_SCALE = 170.0;
static const float WIND_FBM_FREQUENCY = 0.008;

public struct WindSourceGpu
{
    public float4 params;
    public float4 noise;
};

struct B_WindSources
{
    WindSourceGpu data[];
};

[[vk::binding(2, 0)]] ConstantBuffer<U_GuiInput> gui_input;

[[vk::binding(3, 0)]]
readonly GLSLShaderStorageBuffer<B_WindSources, Std430DataLayout> wind_sources;

float2 windSourceOffset(uint sourceIndex)
{
    uint x = sourceIndex * 1664525u + 1013904223u;
    uint y = sourceIndex * 22695477u + 1109515789u;
    return float2(float(x & 1023u) - 512.0, float(y & 1023u) - 512.0);
}

int windSafeOctaves(float octaveValue)
{
    return int(clamp(round(octaveValue), 1.0, 8.0));
}

float sampleWindSourceFbm(float2 samplePosition, float2 timeOffset,
                          uint sourceIndex, float gain, float4 noiseParameters)
{
    float patternScale = max(noiseParameters.x, 0.05);
    int octaves = windSafeOctaves(noiseParameters.y);
    float lacunarity = max(noiseParameters.z, 1.0);
    float persistence = clamp(noiseParameters.w, 0.0, 1.0);
    float2 offset = windSourceOffset(sourceIndex);
    float2 position = (samplePosition + offset + timeOffset) / patternScale;
    float frequency = WIND_FBM_FREQUENCY;
    float amplitude = 1.0;
    float noiseSum = 0.0;

    for (int octave = 0; octave < 8; ++octave)
    {
        if (octave >= octaves)
        {
            break;
        }
        uint seed = WIND_FBM_SEED + sourceIndex * 997u +
                    uint(octave) * 1000u;
        noiseSum += seededClassicGradientNoise2D(
            position * frequency, seed) * amplitude;
        frequency *= lacunarity;
        amplitude *= persistence;
    }

    return noiseSum * max(gain, 0.0);
}

public float3 sampleProceduralWind(float3 worldPosition, float time)
{
    float2 samplePosition = worldPosition.xz * WIND_SAMPLE_SCALE;
    float scrollTime = time * WIND_TIME_SCALE;
    float2 planarWind = float2(0.0);

    for (uint sourceIndex = 0u;
         sourceIndex < gui_input.wind_source_count;
         ++sourceIndex)
    {
        WindSourceGpu gpuSource = wind_sources.data[sourceIndex];
        float4 source = gpuSource.params;
        float4 noiseParameters = gpuSource.noise;
        float directionAngle = radians(source.x);
        float sourceSpeed = max(source.y, 0.0);
        float sourceGain = max(source.z, 0.0);
        if (sourceGain <= EPSILON)
        {
            continue;
        }

        float2 windDirection = float2(
            cos(directionAngle), sin(directionAngle));
        float2 layerTime = -windDirection * scrollTime * sourceSpeed;
        float turbulentFactor = sampleWindSourceFbm(
            samplePosition, layerTime, sourceIndex, sourceGain,
            noiseParameters);
        float windFactor = max(
            0.0,
            sourceGain * max(gui_input.wind_directional_bias_fraction, 0.0) +
            turbulentFactor * max(gui_input.wind_turbulence_fraction, 0.0));
        planarWind += windDirection * windFactor;
    }

    return float3(planarWind.x, 0.0, planarWind.y);
}
