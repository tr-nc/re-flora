module composition_sky;

import composition_types;
import skylight;
import starlight;
import sunlight;
import tracer_types;

static const float PI = 3.14159265358979323846;

float3x3 makeSunBasis(float3 normal, float sunAzimuth)
{
    float3 tangent;
    if (abs(normal.y) < 0.99999)
    {
        tangent = normalize(cross(float3(0.0, 1.0, 0.0), normal));
    }
    else
    {
        float angle = sunAzimuth * 2.0 * PI;
        tangent = float3(cos(angle), 0.0, -sin(angle));
    }
    float3 bitangent = cross(normal, tangent);
    return float3x3(tangent, bitangent, normal);
}

float4 sampleSunSprite(Sampler2D sunSprite, float4x4 viewMatrix,
                       float3 viewDirection, float3 sunDirection, float sunSize)
{
    if (dot(viewDirection, sunDirection) <= 0.0) return float4(0.0);

    float3 viewDirectionVs = normalize(mul(viewMatrix, float4(viewDirection, 0.0)).xyz);
    float3 sunDirectionVs = normalize(mul(viewMatrix, float4(sunDirection, 0.0)).xyz);
    float viewZ = -viewDirectionVs.z;
    float sunZ = -sunDirectionVs.z;
    if (viewZ <= 1.0e-5 || sunZ <= 1.0e-5) return float4(0.0);

    float2 viewProjection = viewDirectionVs.xy / viewZ;
    float2 sunProjection = sunDirectionVs.xy / sunZ;
    float2 uv = (viewProjection - sunProjection) / max(sunSize, 0.0001) + 0.5;
    if (any(uv < float2(0.0)) || any(uv > float2(1.0))) return float4(0.0);
    return sunSprite.SampleLevel(uv, 0.0);
}

public float3 computeSkyWithSunAndStars(float3 viewDirection,
                                        U_SunInfo sun,
                                        U_CameraInfo camera,
                                        U_StarlightInfo starlight,
                                        Sampler2D sunSprite)
{
    float3 skyColor = getSkyColor(viewDirection, sun.sun_dir);
    float4 sunTexel = sampleSunSprite(sunSprite, camera.view_mat, viewDirection,
                                      sun.sun_dir, sun.sun_size);
    float sunDayFactor = sunLuminanceFromDirection(sun.sun_dir, 1.0);
    float3 sunColor = sunTexel.rgb * sun.sun_color;
    sunColor *= sun.sun_display_luminance * sunDayFactor;
    float sunAlpha = sunTexel.a * sunDayFactor;
    float3 skyWithSun = lerp(skyColor, sunColor, sunAlpha);

    const float starCutoffAltitude = -0.05;
    const float starFadeAltitude = -0.4;
    float sunAltitude = sun.sun_dir.y;
    if (sunAltitude > starCutoffAltitude) return skyWithSun;

    StarlightInfo info;
    info.iterations = starlight.iterations;
    info.formuparam = starlight.formuparam;
    info.volsteps = starlight.volsteps;
    info.stepsize = starlight.stepsize;
    info.zoom = starlight.zoom;
    info.tile = starlight.tile;
    info.speed = starlight.speed;
    info.brightness = starlight.brightness;
    info.darkmatter = starlight.darkmatter;
    info.distfading = starlight.distfading;
    info.saturation = starlight.saturation;

    float3 rotatedViewDirection = mul(makeSunBasis(sun.sun_dir, sun.sun_azimuth),
                                      viewDirection);
    float3 starColor = getStarlightColor(rotatedViewDirection, info);
    float starVisibility = smoothstep(starCutoffAltitude, starFadeAltitude, sunAltitude);
    return lerp(skyWithSun, starColor, starVisibility);
}
