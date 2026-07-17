module clouds;

import hash;
import skylight;
import tracer_types;

static const float CLOUD_PI = 3.14159265;

public struct CloudRaymarchResult
{
    public float3 color;
    public float alpha;
};

public struct CloudInputs
{
    public U_GuiInput gui;
    public U_SunInfo sun;
    public U_CameraInfo camera;
    public U_EnvInfo environment;
};

public CloudInputs makeCloudInputs(U_GuiInput gui, U_SunInfo sun,
                                   U_CameraInfo camera, U_EnvInfo environment)
{
    CloudInputs inputs;
    inputs.gui = gui;
    inputs.sun = sun;
    inputs.camera = camera;
    inputs.environment = environment;
    return inputs;
}

float cloudTimeSeconds(CloudInputs inputs)
{
    return float(inputs.environment.frame_serial_idx) * 0.016;
}

float2 cloudWindOffset(CloudInputs inputs, float multiplier)
{
    float2 windDirection = normalize(float2(0.82, 0.31));
    return windDirection * cloudTimeSeconds(inputs) * inputs.gui.cloud_wind_speed * multiplier;
}

float cloudValueNoise2(float2 position)
{
    float2 cell = floor(position);
    float2 fraction = frac(position);
    fraction = fraction * fraction * (3.0 - 2.0 * fraction);

    float a = hash12(cell + float2(0.0, 0.0));
    float b = hash12(cell + float2(1.0, 0.0));
    float c = hash12(cell + float2(0.0, 1.0));
    float d = hash12(cell + float2(1.0, 1.0));
    return lerp(lerp(a, b, fraction.x), lerp(c, d, fraction.x), fraction.y);
}

float cloudValueNoise3(float3 position)
{
    float3 cell = floor(position);
    float3 fraction = frac(position);
    fraction = fraction * fraction * (3.0 - 2.0 * fraction);

    float n000 = hash13(cell + float3(0.0, 0.0, 0.0));
    float n100 = hash13(cell + float3(1.0, 0.0, 0.0));
    float n010 = hash13(cell + float3(0.0, 1.0, 0.0));
    float n110 = hash13(cell + float3(1.0, 1.0, 0.0));
    float n001 = hash13(cell + float3(0.0, 0.0, 1.0));
    float n101 = hash13(cell + float3(1.0, 0.0, 1.0));
    float n011 = hash13(cell + float3(0.0, 1.0, 1.0));
    float n111 = hash13(cell + float3(1.0, 1.0, 1.0));

    float x00 = lerp(n000, n100, fraction.x);
    float x10 = lerp(n010, n110, fraction.x);
    float x01 = lerp(n001, n101, fraction.x);
    float x11 = lerp(n011, n111, fraction.x);
    float y0 = lerp(x00, x10, fraction.y);
    float y1 = lerp(x01, x11, fraction.y);
    return lerp(y0, y1, fraction.z);
}

float cloudFbm2(float2 position)
{
    float sum = 0.0;
    float amplitude = 0.58;
    sum += cloudValueNoise2(position) * amplitude;
    position *= 2.03;
    amplitude *= 0.50;
    sum += cloudValueNoise2(position + float2(17.2, 9.4)) * amplitude;
    return clamp(sum / 0.87, 0.0, 1.0);
}

float cloudFbm3Shape(float3 position)
{
    float sum = 0.0;
    float amplitude = 0.58;
    sum += cloudValueNoise3(position) * amplitude;
    position *= 2.03;
    amplitude *= 0.50;
    sum += cloudValueNoise3(position + float3(19.1, 3.7, 11.3)) * amplitude;
    position *= 2.01;
    amplitude *= 0.42;
    sum += cloudValueNoise3(position + float3(5.6, 23.9, 41.1)) * amplitude;
    return clamp(sum / 1.29, 0.0, 1.0);
}

public float cloudLayerBottom(CloudInputs inputs)
{
    return min(inputs.gui.cloud_bottom_height, inputs.gui.cloud_top_height - 1.0);
}

public float cloudLayerTop(CloudInputs inputs)
{
    return max(inputs.gui.cloud_top_height, inputs.gui.cloud_bottom_height + 1.0);
}

float cloudHeightFraction(CloudInputs inputs, float3 worldPosition)
{
    float bottom = cloudLayerBottom(inputs);
    float top = cloudLayerTop(inputs);
    return clamp((worldPosition.y - bottom) / max(top - bottom, 1.0), 0.0, 1.0);
}

float cloudHeightGradient(CloudInputs inputs, float3 worldPosition)
{
    float height = cloudHeightFraction(inputs, worldPosition);
    float bottom = smoothstep(0.00, 0.16, height);
    float top = 1.0 - smoothstep(0.72, 1.00, height);
    float anvil = smoothstep(0.18, 0.52, height) *
                  (1.0 - smoothstep(0.86, 1.0, height));
    float wisps = smoothstep(0.02, 0.10, height) *
                  (1.0 - smoothstep(0.18, 0.38, height));
    return clamp(bottom * top * lerp(0.70, 1.15, anvil) + wisps * 0.20, 0.0, 1.0);
}

float cloudCoverageAt(CloudInputs inputs, float2 worldXz)
{
    float2 weatherPosition = worldXz * 0.0045 + cloudWindOffset(inputs, 0.012);
    float weather = cloudFbm2(weatherPosition);
    float coverage = inputs.gui.cloud_coverage * lerp(0.58, 1.35, weather);
    return clamp(coverage, 0.0, 1.0);
}

public float cloudDensityLowQuality(CloudInputs inputs, float3 worldPosition)
{
    float vertical = cloudHeightGradient(inputs, worldPosition);
    if (vertical <= 0.0) return 0.0;

    float coverage = cloudCoverageAt(inputs, worldPosition.xz);
    if (coverage <= 0.001 || inputs.gui.cloud_density <= 0.001) return 0.0;

    float2 wind = cloudWindOffset(inputs, 0.055);
    float3 position = float3(worldPosition.xz * inputs.gui.cloud_shape_scale + wind,
                             worldPosition.y * inputs.gui.cloud_shape_scale * 1.8);
    float shape = cloudValueNoise3(position);
    float threshold = lerp(0.78, 0.24, coverage);
    float density = smoothstep(threshold, threshold + 0.15, shape);
    return density * vertical * inputs.gui.cloud_density;
}

float cloudDensity(CloudInputs inputs, float3 worldPosition)
{
    float vertical = cloudHeightGradient(inputs, worldPosition);
    if (vertical <= 0.0) return 0.0;

    float coverage = cloudCoverageAt(inputs, worldPosition.xz);
    if (coverage <= 0.001 || inputs.gui.cloud_density <= 0.001) return 0.0;

    float2 wind = cloudWindOffset(inputs, 0.055);
    float3 shapePosition = float3(
        worldPosition.xz * inputs.gui.cloud_shape_scale + wind,
        worldPosition.y * inputs.gui.cloud_shape_scale * 1.8);
    float shape = cloudFbm3Shape(shapePosition);
    float threshold = lerp(0.76, 0.22, coverage);
    float baseDensity = smoothstep(threshold, threshold + 0.14, shape);

    float2 detailWind = cloudWindOffset(inputs, 0.17);
    float3 detailPosition = float3(
        worldPosition.xz * inputs.gui.cloud_detail_scale + detailWind,
        worldPosition.y * inputs.gui.cloud_detail_scale * 2.4 + 37.0);
    float detail = cloudValueNoise3(detailPosition);
    float edgeWeight = 1.0 - smoothstep(0.20, 0.72, baseDensity);
    float lowerWisp = 1.0 - smoothstep(0.24, 0.58,
                                      cloudHeightFraction(inputs, worldPosition));
    float erosion = detail * inputs.gui.cloud_detail_strength *
                    (0.46 + 0.44 * lowerWisp) * edgeWeight;

    float density = max(baseDensity - erosion, 0.0);
    density = min(density * 1.10, 1.0);
    density *= vertical * inputs.gui.cloud_density;
    return max(density, 0.0);
}

bool cloudLayerInterval(CloudInputs inputs, float3 viewDirection,
                        out float startDistance, out float endDistance)
{
    startDistance = 0.0;
    endDistance = 0.0;
    if (inputs.gui.clouds_enabled == 0u || inputs.gui.cloud_coverage <= 0.001 ||
        inputs.gui.cloud_density <= 0.001)
    {
        return false;
    }
    if (abs(viewDirection.y) < 0.018) return false;

    float bottom = cloudLayerBottom(inputs);
    float top = cloudLayerTop(inputs);
    float bottomDistance = (bottom - inputs.camera.pos.y) / viewDirection.y;
    float topDistance = (top - inputs.camera.pos.y) / viewDirection.y;
    startDistance = max(min(bottomDistance, topDistance), 0.0);
    endDistance = max(bottomDistance, topDistance);
    if (endDistance <= startDistance) return false;

    endDistance = min(endDistance, max(inputs.gui.cloud_max_distance, 1.0));
    return endDistance > startDistance;
}

float cloudHorizonDip(float3 worldPosition)
{
    float height = max(worldPosition.y, 0.0);
    return clamp(sqrt(height * 0.000035), 0.015, 0.11);
}

float cloudDirectSunVisibility(CloudInputs inputs, float3 worldPosition)
{
    float dip = cloudHorizonDip(worldPosition);
    return smoothstep(-dip - 0.03, -dip * 0.20 + 0.015, inputs.sun.sun_dir.y);
}

float3 cloudAtmosphericSunTransmittance(CloudInputs inputs, float3 worldPosition)
{
    float dip = cloudHorizonDip(worldPosition);
    float effectiveSineAltitude = clamp(inputs.sun.sun_dir.y + dip + 0.025, 0.025, 1.0);
    float airMass = clamp(1.0 / effectiveSineAltitude, 1.0, 28.0);
    float3 beta = float3(0.030, 0.075, 0.200);
    return exp(-beta * max(airMass - 1.0, 0.0));
}

float cloudLightExitDistance(CloudInputs inputs, float3 worldPosition, float3 lightDirection)
{
    float bottom = cloudLayerBottom(inputs);
    float top = cloudLayerTop(inputs);
    float layerThickness = max(top - bottom, 1.0);

    float pathLength;
    if (abs(lightDirection.y) < 0.012)
    {
        pathLength = layerThickness * 8.0;
    }
    else
    {
        float exitHeight = lightDirection.y > 0.0 ? top : bottom;
        pathLength = max((exitHeight - worldPosition.y) / lightDirection.y, 0.0);
    }

    float grazing = 1.0 - smoothstep(0.05, 0.32, abs(lightDirection.y));
    float pathCap = layerThickness * lerp(4.0, 9.0, grazing);
    return min(pathLength, pathCap);
}

float cloudSunTransmittance(CloudInputs inputs, float3 worldPosition,
                            float localDensity, float directVisibility)
{
    if (directVisibility <= 0.001) return 0.0;

    float pathLength = cloudLightExitDistance(inputs, worldPosition, inputs.sun.sun_dir);
    float opticalDepth = localDensity * pathLength * 0.012;
    uint lightSteps = clamp(inputs.gui.cloud_light_steps, 1u, 8u);
    if (lightSteps > 1u && pathLength > 0.001)
    {
        float probeDistance = pathLength / float(lightSteps);
        float3 probePosition = worldPosition;
        for (uint step = 1u; step < lightSteps; ++step)
        {
            probePosition += inputs.sun.sun_dir * probeDistance;
            opticalDepth += cloudDensityLowQuality(inputs, probePosition) *
                            probeDistance * 0.016;
        }
    }
    return exp(-opticalDepth * inputs.gui.cloud_absorption);
}

float cloudHenyeyGreensteinPhase(float cosine, float eccentricity)
{
    float eccentricitySquared = eccentricity * eccentricity;
    float denominator = max(1.0 + eccentricitySquared - 2.0 * eccentricity * cosine,
                            1.0e-3);
    return (1.0 - eccentricitySquared) /
           (4.0 * CLOUD_PI * denominator * sqrt(denominator));
}

float3 cloudSampleLighting(CloudInputs inputs, float3 worldPosition,
                           float3 viewDirection, float density)
{
    float directVisibility = cloudDirectSunVisibility(inputs, worldPosition);
    float3 atmosphereTransmittance = cloudAtmosphericSunTransmittance(inputs, worldPosition);
    float sunTransmittance = cloudSunTransmittance(inputs, worldPosition, density,
                                                   directVisibility);
    float cosine = clamp(dot(viewDirection, inputs.sun.sun_dir), -1.0, 1.0);
    float eccentricity = clamp(inputs.gui.cloud_phase_eccentricity, 0.0, 0.92);
    float forwardPhase = cloudHenyeyGreensteinPhase(cosine, eccentricity) /
                         max(cloudHenyeyGreensteinPhase(1.0, eccentricity), 1.0e-3);
    float backPhase = cloudHenyeyGreensteinPhase(cosine, -0.22) /
                      max(cloudHenyeyGreensteinPhase(-1.0, -0.22), 1.0e-3);
    float phase = 0.32 + 1.55 * forwardPhase + 0.25 * backPhase;

    float powder = 1.0 - exp(-density * 2.2);
    float silver = pow(max(cosine, 0.0), 18.0) * inputs.gui.cloud_silver_intensity;
    float3 ambientSky = getSkyColor(
        normalize(float3(viewDirection.x, abs(viewDirection.y) + 0.25, viewDirection.z)),
        inputs.sun.sun_dir);
    float twilight = smoothstep(-0.08, 0.22, inputs.sun.sun_dir.y);
    float3 ambient = ambientSky * lerp(0.38, 0.62, twilight) *
                     lerp(0.55, 1.0, clamp(directVisibility, 0.0, 1.0));

    float3 incidentSun = inputs.sun.sun_color * atmosphereTransmittance * directVisibility;
    float3 sunLight = incidentSun * inputs.sun.sun_luminance * sunTransmittance;
    sunLight *= phase * (0.52 + 0.55 * powder);
    sunLight += incidentSun * inputs.sun.sun_display_luminance * sunTransmittance * silver;
    return ambient + sunLight;
}

public CloudRaymarchResult computeVolumetricCloudsWithJitter(
    CloudInputs inputs, float3 viewDirection, uint primaryStepOverride, float jitter)
{
    CloudRaymarchResult result;
    result.color = float3(0.0);
    result.alpha = 0.0;

    float startDistance;
    float endDistance;
    if (!cloudLayerInterval(inputs, viewDirection, startDistance, endDistance)) return result;

    uint configuredSteps = clamp(inputs.gui.cloud_primary_steps, 4u, 64u);
    uint steps = primaryStepOverride == 0u
                     ? configuredSteps
                     : clamp(primaryStepOverride, 3u, 64u);
    float marchLength = endDistance - startDistance;
    float stepDistance = marchLength / float(steps);
    float distance = startDistance + stepDistance * frac(jitter);
    float transmittance = 1.0;

    for (uint step = 0u; step < steps; ++step)
    {
        float3 worldPosition = inputs.camera.pos.xyz + viewDirection * distance;
        float density = cloudDensity(inputs, worldPosition);
        if (density > 0.001)
        {
            float extinction = density * inputs.gui.cloud_absorption;
            float alpha = 1.0 - exp(-extinction * stepDistance * 0.105);
            alpha = clamp(alpha, 0.0, 0.96);
            float3 lighting = cloudSampleLighting(inputs, worldPosition,
                                                  viewDirection, density);
            result.color += transmittance * alpha * lighting;
            transmittance *= 1.0 - alpha;
            if (transmittance < 0.035) break;
        }
        distance += stepDistance;
    }

    result.alpha = clamp(1.0 - transmittance, 0.0, 1.0);
    return result;
}

public CloudRaymarchResult computeVolumetricClouds(CloudInputs inputs,
                                                    float3 viewDirection,
                                                    uint primaryStepOverride)
{
    float jitter = hash13(float3(
        viewDirection.xy * 173.31 + viewDirection.zz * 41.7,
        float(inputs.environment.frame_serial_idx & 63u)));
    return computeVolumetricCloudsWithJitter(inputs, viewDirection,
                                             primaryStepOverride, jitter);
}

public float3 applyCloudResult(float3 baseSkyColor, CloudRaymarchResult clouds)
{
    return lerp(baseSkyColor, clouds.color, clouds.alpha);
}
