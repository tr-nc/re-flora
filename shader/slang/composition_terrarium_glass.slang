module composition_terrarium_glass;

import composition_cloud_reflection;
import composition_scene;
import composition_sky;
import composition_types;
import composition_terrarium_panels;
import hash;
import projection;
import ray;
import tracer_types;

public struct TerrariumGlassHit
{
    public bool hit;
    public float t;
    public float3 position;
    public float3 normal;
    public float2 face_uv;
};

public struct TerrariumGlassUniforms
{
    public U_GuiInput gui;
    public U_SunInfo sun;
    public U_CameraInfo camera;
    public U_EnvInfo environment;
    public U_StarlightInfo starlight;
};

public TerrariumGlassUniforms makeTerrariumGlassUniforms(
    U_GuiInput gui, U_SunInfo sun, U_CameraInfo camera,
    U_EnvInfo environment, U_StarlightInfo starlight)
{
    TerrariumGlassUniforms inputs;
    inputs.gui = gui;
    inputs.sun = sun;
    inputs.camera = camera;
    inputs.environment = environment;
    inputs.starlight = starlight;
    return inputs;
}

float3 sampleSceneColor(
    float2 screenUv, TerrariumGlassUniforms inputs,
    Sampler2D rasterOutput, Sampler2D rasterDepthTexture,
    readonly RWTexture2D<float4> terrainOutput,
    readonly RWTexture2D<float> computeDepthTexture,
    RWTexture2D<float4> compositedTexture,
    Sampler2D sunSprite, Sampler2D cloudOutput)
{
    uint width;
    uint height;
    compositedTexture.GetDimensions(width, height);
    int2 imageSize = int2(width, height);
    float2 clampedUv = clamp(screenUv, float2(0.0), float2(1.0));
    int2 coordinate = int2(clamp(clampedUv * float2(imageSize), float2(0.0),
                                 float2(imageSize - int2(1))));

    float rasterDepth = rasterDepthTexture.Load(int3(coordinate, 0)).r;
    float computeDepth = computeDepthTexture[coordinate];
    float4 rasterColor = rasterOutput.Load(int3(coordinate, 0));
    float3 terrainColor = terrainOutput[coordinate].rgb;
    return combineSceneColors(rasterDepth, computeDepth, rasterColor,
                              terrainColor, clampedUv, inputs.gui,
                              inputs.sun, inputs.camera, inputs.starlight,
                              sunSprite, cloudOutput);
}

float4 sampleRasterColorBilinear(Sampler2D rasterOutput,
                                 float2 sampleUv, int2 textureSize)
{
    float2 maximumTexel = float2(textureSize - int2(1));
    float2 texelPosition = clamp(sampleUv * float2(textureSize) - 0.5,
                                 float2(0.0), maximumTexel);
    int2 first = int2(floor(texelPosition));
    int2 second = min(first + int2(1), textureSize - int2(1));
    float2 fraction = frac(texelPosition);

    float4 color00 = rasterOutput.Load(int3(first, 0));
    float4 color10 = rasterOutput.Load(int3(int2(second.x, first.y), 0));
    float4 color01 = rasterOutput.Load(int3(int2(first.x, second.y), 0));
    float4 color11 = rasterOutput.Load(int3(second, 0));
    return lerp(lerp(color00, color10, fraction.x),
                lerp(color01, color11, fraction.x), fraction.y);
}

void accumulateRasterReflectionTap(
    inout float3 colorSum, inout float weightSum, inout float totalWeight,
    Sampler2D rasterOutput, float2 sampleUv, int2 textureSize, float weight)
{
    totalWeight += weight;
    float4 rasterColor = sampleRasterColorBilinear(rasterOutput, sampleUv, textureSize);
    if (rasterColor.a <= 0.02) return;

    float tapWeight = weight * clamp(rasterColor.a, 0.0, 1.0);
    colorSum += (rasterColor.rgb / max(rasterColor.a, 0.08)) * tapWeight;
    weightSum += tapWeight;
}

float3 sampleRasterReflectionFootprint(
    U_GuiInput gui, Sampler2D rasterOutput, float2 sampleUv,
    int2 textureSize, float4 centerColor, out float coverage)
{
    float2 texelSize = 1.0 / float2(textureSize);
    float footprintPixels = max(gui.glass_ssr_footprint_pixels, 0.0);
    float3 colorSum = float3(0.0);
    float weightSum = 0.0;
    float totalWeight = 0.0;

    if (footprintPixels <= 0.25)
    {
        accumulateRasterReflectionTap(colorSum, weightSum, totalWeight,
                                      rasterOutput, sampleUv, textureSize, 1.0);
    }
    else
    {
        float radiusPixels = footprintPixels;
        float sigma = max(radiusPixels * 0.42, 0.75);
        for (int y = -2; y <= 2; y++)
        {
            for (int x = -2; x <= 2; x++)
            {
                float2 pixelOffset = float2(float(x), float(y)) * (radiusPixels * 0.5);
                float weight = exp(-0.5 * dot(pixelOffset, pixelOffset) /
                                   (sigma * sigma));
                accumulateRasterReflectionTap(
                    colorSum, weightSum, totalWeight, rasterOutput,
                    sampleUv + pixelOffset * texelSize, textureSize, weight);
            }
        }
    }

    coverage = totalWeight > 0.0 ? clamp(weightSum / totalWeight, 0.0, 1.0) : 0.0;
    if (weightSum > 0.0) return colorSum / weightSum;
    return centerColor.rgb / max(centerColor.a, 0.08);
}

float sampleRasterSsrReflection(
    float3 originWorld, float3 directionWorld, float jitterSeed,
    out float3 reflectionColor, U_GuiInput gui, U_CameraInfo camera,
    Sampler2D rasterOutput, Sampler2D rasterDepthTexture)
{
    reflectionColor = float3(0.0);
    float3 rayDirection = normalize(directionWorld);
    uint stepCount = clamp(gui.glass_ssr_steps, 1u, TERRARIUM_GLASS_SSR_MAX_STEPS);
    float stepLength = TERRARIUM_GLASS_SSR_MAX_DISTANCE_WORLD / float(stepCount);
    float minimumHitThickness = max(gui.glass_ssr_min_hit_thickness_voxels, 0.0) *
                                TERRARIUM_VOXEL_WORLD_SIZE;
    float jitter = clamp(jitterSeed, 0.0, 0.9999) * stepLength;
    uint textureWidth;
    uint textureHeight;
    uint mipLevels;
    rasterOutput.GetDimensions(0, textureWidth, textureHeight, mipLevels);
    int2 textureSize = int2(textureWidth, textureHeight);
    bool hasPreviousDepthSample = false;
    float previousDepthGap = 0.0;
    float previousDistance = 0.0;
    float2 previousUv = float2(0.0);

    for (uint step = 0u; step < TERRARIUM_GLASS_SSR_MAX_STEPS; step++)
    {
        if (step >= stepCount) break;
        float distance = float(step) * stepLength + jitter;
        if (distance <= 0.0) continue;

        float3 probeWorld = originWorld + rayDirection * distance;
        float4 clip = mul(camera.view_proj_mat, float4(probeWorld, 1.0));
        if (clip.w <= 0.0)
        {
            hasPreviousDepthSample = false;
            continue;
        }

        float3 ndc = clip.xyz / clip.w;
        if (ndc.z <= 0.0 || ndc.z >= 1.0)
        {
            hasPreviousDepthSample = false;
            continue;
        }

        float2 sampleUv = ndc.xy * 0.5 + 0.5;
        if (any(sampleUv < float2(0.0)) || any(sampleUv > float2(1.0)))
        {
            hasPreviousDepthSample = false;
            continue;
        }

        int2 coordinate = int2(clamp(sampleUv * float2(textureSize), float2(0.0),
                                     float2(textureSize - int2(1))));
        float rasterDepth = rasterDepthTexture.Load(int3(coordinate, 0)).r;
        float4 rasterColor = rasterOutput.Load(int3(coordinate, 0));
        if (rasterDepth >= 1.0 || rasterColor.a <= 0.02)
        {
            hasPreviousDepthSample = false;
            continue;
        }

        Ray cameraRay = generateCameraRay(sampleUv, camera.view_proj_mat_inv);
        float sceneCameraDistance = sceneRayDistanceFromDepth(
            sampleUv, cameraRay, rasterDepth, camera);
        float probeCameraDistance = dot(probeWorld - cameraRay.origin,
                                        cameraRay.direction);
        float depthGap = probeCameraDistance - sceneCameraDistance;
        float hitThickness = max(minimumHitThickness, 1.0e-5) + distance * 0.028;
        bool crossesDepth = hasPreviousDepthSample &&
                            previousDepthGap < 0.0 && depthGap >= 0.0;
        bool insideDepthSlab = depthGap >= 0.0 && depthGap <= hitThickness;
        if (!crossesDepth && !insideDepthSlab)
        {
            hasPreviousDepthSample = true;
            previousDepthGap = depthGap;
            previousDistance = distance;
            previousUv = sampleUv;
            continue;
        }

        float2 hitUv = sampleUv;
        float hitDistance = distance;
        float depthFade = 1.0 - clamp(depthGap / hitThickness, 0.0, 1.0);
        if (crossesDepth)
        {
            float crossAlpha = clamp((-previousDepthGap) /
                                     max(depthGap - previousDepthGap, 1.0e-5),
                                     0.0, 1.0);
            hitUv = lerp(previousUv, sampleUv, crossAlpha);
            hitDistance = lerp(previousDistance, distance, crossAlpha);
            depthFade = 1.0;
        }

        float4 hitColor = sampleRasterColorBilinear(rasterOutput, hitUv, textureSize);
        if (hitColor.a <= 0.02)
        {
            hasPreviousDepthSample = true;
            previousDepthGap = depthGap;
            previousDistance = distance;
            previousUv = sampleUv;
            continue;
        }

        float screenEdge = min(min(hitUv.x, 1.0 - hitUv.x),
                               min(hitUv.y, 1.0 - hitUv.y));
        float screenFade = smoothstep(0.02, 0.14, screenEdge);
        float distanceFade = 1.0 - smoothstep(
            TERRARIUM_GLASS_SSR_MAX_DISTANCE_WORLD * 0.72,
            TERRARIUM_GLASS_SSR_MAX_DISTANCE_WORLD, hitDistance);
        float confidence = screenFade * distanceFade * depthFade *
                           clamp(hitColor.a, 0.0, 1.0);
        float footprintCoverage;
        reflectionColor = sampleRasterReflectionFootprint(
            gui, rasterOutput, hitUv, textureSize, hitColor, footprintCoverage);
        return confidence * footprintCoverage;
    }
    return 0.0;
}

TerrariumGlassHit makeEmptyTerrariumGlassHit()
{
    TerrariumGlassHit hit;
    hit.hit = false;
    hit.t = 1.0e20;
    hit.position = float3(0.0);
    hit.normal = float3(0.0, 0.0, 1.0);
    hit.face_uv = float2(0.0);
    return hit;
}

void storeTerrariumGlassHit(inout TerrariumGlassHit bestHit, float distance,
                            float3 position, float3 normal, float2 faceUv,
                            U_CameraInfo camera)
{
    float4 clip = mul(camera.view_proj_mat, float4(position, 1.0));
    if (clip.w <= 0.0) return;

    bestHit.hit = true;
    bestHit.t = distance;
    bestHit.position = position;
    bestHit.normal = normal;
    bestHit.face_uv = clamp(faceUv, float2(0.0), float2(1.0));
}

void tryVerticalGlassFace(inout TerrariumGlassHit bestHit, Ray ray,
                          float3 normal, float planeCoordinate, int axis,
                          float minimumDistance, U_CameraInfo camera)
{
    float denominator = axis == 0 ? ray.direction.x : ray.direction.z;
    if (abs(denominator) < 1.0e-5) return;

    float originCoordinate = axis == 0 ? ray.origin.x : ray.origin.z;
    float distance = (planeCoordinate - originCoordinate) / denominator;
    if (distance <= minimumDistance || distance >= bestHit.t) return;

    float3 position = ray.origin + ray.direction * distance;
    if (position.y < TERRARIUM_GLASS_BOX_MIN.y ||
        position.y > TERRARIUM_GLASS_BOX_MAX.y)
    {
        return;
    }

    float2 faceUv;
    if (axis == 0)
    {
        if (position.z < TERRARIUM_GLASS_BOX_MIN.z ||
            position.z > TERRARIUM_GLASS_BOX_MAX.z)
        {
            return;
        }
        faceUv = float2(
            (position.z - TERRARIUM_GLASS_BOX_MIN.z) /
                (TERRARIUM_GLASS_BOX_MAX.z - TERRARIUM_GLASS_BOX_MIN.z),
            (position.y - TERRARIUM_GLASS_BOX_MIN.y) /
                (TERRARIUM_GLASS_BOX_MAX.y - TERRARIUM_GLASS_BOX_MIN.y));
    }
    else
    {
        if (position.x < TERRARIUM_GLASS_BOX_MIN.x ||
            position.x > TERRARIUM_GLASS_BOX_MAX.x)
        {
            return;
        }
        faceUv = float2(
            (position.x - TERRARIUM_GLASS_BOX_MIN.x) /
                (TERRARIUM_GLASS_BOX_MAX.x - TERRARIUM_GLASS_BOX_MIN.x),
            (position.y - TERRARIUM_GLASS_BOX_MIN.y) /
                (TERRARIUM_GLASS_BOX_MAX.y - TERRARIUM_GLASS_BOX_MIN.y));
    }
    storeTerrariumGlassHit(bestHit, distance, position, normal, faceUv, camera);
}

TerrariumGlassHit traceTerrariumGlassAfter(Ray ray, float minimumDistance,
                                           U_CameraInfo camera)
{
    TerrariumGlassHit hit = makeEmptyTerrariumGlassHit();
    tryVerticalGlassFace(hit, ray, float3(-1.0, 0.0, 0.0),
                         TERRARIUM_GLASS_BOX_MIN.x, 0, minimumDistance, camera);
    tryVerticalGlassFace(hit, ray, float3(1.0, 0.0, 0.0),
                         TERRARIUM_GLASS_BOX_MAX.x, 0, minimumDistance, camera);
    tryVerticalGlassFace(hit, ray, float3(0.0, 0.0, -1.0),
                         TERRARIUM_GLASS_BOX_MIN.z, 1, minimumDistance, camera);
    tryVerticalGlassFace(hit, ray, float3(0.0, 0.0, 1.0),
                         TERRARIUM_GLASS_BOX_MAX.z, 1, minimumDistance, camera);
    return hit;
}

float3 shadeTerrariumGlassHit(
    float3 sceneColor, float2 screenUv, Ray ray, TerrariumGlassHit hit,
    TerrariumGlassUniforms inputs,
    Sampler2D rasterOutput, Sampler2D rasterDepthTexture,
    readonly RWTexture2D<float4> terrainOutput,
    readonly RWTexture2D<float> computeDepthTexture,
    RWTexture2D<float4> compositedTexture,
    Sampler2D sunSprite, Sampler2D cloudOutput)
{
    float3 viewDirection = normalize(-ray.direction);
    float3 normal = hit.normal;
    if (dot(normal, viewDirection) < 0.0) normal = -normal;
    if (dot(ray.direction, hit.normal) <= 0.0) return sceneColor;

    float3 configuredTint = clamp(inputs.gui.glass_tint, float3(0.0), float3(1.5));
    float reflectionStrength = max(inputs.gui.glass_reflection_strength, 0.0);
    float ssrStrength = max(inputs.gui.glass_ssr_strength, 0.0);
    float refractionStrength = max(inputs.gui.glass_refraction_strength, 0.0);
    float alphaMultiplier = max(inputs.gui.glass_alpha, 0.0);
    float glintStrength = max(inputs.gui.glass_glint_strength, 0.0);
    bool usePerVoxelReflection = inputs.gui.glass_per_voxel_reflection != 0u;

    float2 uv = hit.face_uv;
    float2 faceWorldExtent = abs(hit.normal.y) > 0.5
        ? float2(TERRARIUM_GLASS_BOX_MAX.x - TERRARIUM_GLASS_BOX_MIN.x,
                 TERRARIUM_GLASS_BOX_MAX.z - TERRARIUM_GLASS_BOX_MIN.z)
        : (abs(hit.normal.x) > 0.5
            ? float2(TERRARIUM_GLASS_BOX_MAX.z - TERRARIUM_GLASS_BOX_MIN.z,
                     TERRARIUM_GLASS_BOX_MAX.y - TERRARIUM_GLASS_BOX_MIN.y)
            : float2(TERRARIUM_GLASS_BOX_MAX.x - TERRARIUM_GLASS_BOX_MIN.x,
                     TERRARIUM_GLASS_BOX_MAX.y - TERRARIUM_GLASS_BOX_MIN.y));
    float2 edgeWidth = TERRARIUM_GLASS_THICKNESS_WORLD / faceWorldExtent;
    float2 coreWidth = TERRARIUM_GLASS_CORE_THICKNESS_WORLD / faceWorldExtent;
    float leftEdge = 1.0 - step(edgeWidth.x, uv.x);
    float rightEdge = step(1.0 - edgeWidth.x, uv.x);
    float bottomEdge = 1.0 - step(edgeWidth.y, uv.y);
    float topEdge = step(1.0 - edgeWidth.y, uv.y);
    float edge = max(max(leftEdge, rightEdge), max(bottomEdge, topEdge));
    float pane = 1.0 - edge;

    float coreLeftEdge = 1.0 - step(coreWidth.x, uv.x);
    float coreRightEdge = step(1.0 - coreWidth.x, uv.x);
    float coreBottomEdge = 1.0 - step(coreWidth.y, uv.y);
    float coreTopEdge = step(1.0 - coreWidth.y, uv.y);
    float coreEdge = max(max(coreLeftEdge, coreRightEdge),
                         max(coreBottomEdge, coreTopEdge));
    float corner = max(leftEdge, rightEdge) * max(topEdge, bottomEdge);

    float2 voxelGridExtent = faceWorldExtent * TERRARIUM_GLASS_VOXEL_GRID;
    float2 voxelPosition = uv * voxelGridExtent;
    float2 voxelCell = floor(voxelPosition);
    float2 voxelUv = frac(voxelPosition);
    float2 voxelCenterUv = clamp((voxelCell + 0.5) /
                                 max(voxelGridExtent, float2(1.0)),
                                 float2(0.0), float2(1.0));
    float2 voxelCenterOffset = voxelUv - 0.5;
    float cellNoise = hash13(float3(voxelCell,
        dot(abs(hit.normal), float3(3.0, 17.0, 41.0))));
    float cellNoiseB = hash13(float3(voxelCell.yx + float2(29.0, 11.0),
        dot(abs(hit.normal), float3(53.0, 7.0, 23.0))));
    float cellNoiseC = hash13(float3(voxelCell + float2(5.0, 31.0), 71.0));
    float voxelLens = dot(voxelCenterOffset, voxelCenterOffset) * pane;
    float2 voxelSlope = voxelCenterOffset * (0.45 + cellNoiseC * 0.22);
    float3 faceTangent = abs(hit.normal.x) > 0.5
                             ? float3(0.0, 0.0, 1.0)
                             : float3(1.0, 0.0, 0.0);
    float3 faceBitangent = float3(0.0, 1.0, 0.0);
    float3 microNormal = normalize(
        normal + faceTangent * ((cellNoise - 0.5) * 0.050 + voxelSlope.x * 0.030) +
        faceBitangent * ((cellNoiseB - 0.5) * 0.040 + voxelSlope.y * 0.026));
    float3 voxelReflectionNormal = normalize(
        normal + faceTangent * ((cellNoise - 0.5) * 0.050) +
        faceBitangent * ((cellNoiseB - 0.5) * 0.040));

    float3 voxelCenterWorld = hit.position;
    if (abs(hit.normal.x) > 0.5)
    {
        voxelCenterWorld.z = lerp(TERRARIUM_GLASS_BOX_MIN.z,
                                  TERRARIUM_GLASS_BOX_MAX.z, voxelCenterUv.x);
        voxelCenterWorld.y = lerp(TERRARIUM_GLASS_BOX_MIN.y,
                                  TERRARIUM_GLASS_BOX_MAX.y, voxelCenterUv.y);
    }
    else
    {
        voxelCenterWorld.x = lerp(TERRARIUM_GLASS_BOX_MIN.x,
                                  TERRARIUM_GLASS_BOX_MAX.x, voxelCenterUv.x);
        voxelCenterWorld.y = lerp(TERRARIUM_GLASS_BOX_MIN.y,
                                  TERRARIUM_GLASS_BOX_MAX.y, voxelCenterUv.y);
    }

    float3 reflectionOriginWorld = usePerVoxelReflection
                                       ? voxelCenterWorld : hit.position;
    float3 reflectionViewDirection = usePerVoxelReflection
        ? normalize(inputs.camera.pos.xyz - voxelCenterWorld) : viewDirection;
    float3 reflectionNormal = usePerVoxelReflection
                                  ? voxelReflectionNormal : normal;
    float mirrorFresnel = pow(1.0 - clamp(dot(reflectionNormal,
                                              reflectionViewDirection), 0.0, 1.0), 5.0);
    float reflectionVoxelLens = usePerVoxelReflection
                                    ? pane * (0.12 + cellNoiseC * 0.08)
                                    : mirrorFresnel;

    float normalView = clamp(dot(microNormal, viewDirection), 0.0, 1.0);
    float fresnel = pow(1.0 - normalView, 5.0);
    float fresnelBand = step(0.42, fresnel);

    float3 entryNormal = dot(ray.direction, hit.normal) < 0.0
                             ? microNormal : -microNormal;
    float eta = dot(ray.direction, hit.normal) < 0.0 ? 1.0 / 1.45 : 1.45;
    float3 physicalRefractedDirection = refract(ray.direction, entryNormal, eta);
    if (dot(physicalRefractedDirection, physicalRefractedDirection) < 1.0e-5)
        physicalRefractedDirection = ray.direction;
    float2 physicalRefractionUv = projectWorldToScreen(
        hit.position + physicalRefractedDirection *
            (0.20 + edge * 0.24 + coreEdge * 0.12 + voxelLens * 0.018),
        inputs.camera.view_proj_mat);
    float2 physicalOffset = clamp(physicalRefractionUv - screenUv,
                                  float2(-0.027), float2(0.027)) *
                            refractionStrength;

    float3 normalViewSpace = normalize(
        mul(inputs.camera.view_mat, float4(microNormal, 0.0)).xyz);
    float2 normalViewXy = normalViewSpace.xy;
    float2 uvEdgePush = float2(leftEdge - rightEdge, bottomEdge - topEdge);
    float2 refractionDirection = normalViewXy * 0.70 + uvEdgePush * 0.50 +
                                 voxelSlope * pane * 0.11;
    if (dot(refractionDirection, refractionDirection) < 1.0e-5)
        refractionDirection = edge > 0.0 ? float2(0.0, -1.0) : float2(0.0);
    else
        refractionDirection = normalize(refractionDirection);

    float paneDistortion = pane * (0.00075 + fresnelBand * 0.00055 +
                                   cellNoise * 0.00048 + voxelLens * 0.00055);
    float edgeDistortion = edge * (0.0090 + coreEdge * 0.0045 +
                                   corner * 0.0040 + fresnelBand * 0.0025);
    float distortion = (paneDistortion + edgeDistortion) * refractionStrength;
    float2 refractionOffset = pane * physicalOffset * (0.17 + voxelLens * 0.035) +
                              edge * physicalOffset * 0.62 +
                              refractionDirection * distortion;
    float2 refractedUv = clamp(screenUv + refractionOffset,
                               float2(0.0), float2(1.0));
    float3 refractedColor = sampleSceneColor(
        refractedUv, inputs, rasterOutput, rasterDepthTexture,
        terrainOutput, computeDepthTexture, compositedTexture,
        sunSprite, cloudOutput);

    float2 chromaticDirection = refractionDirection;
    if (dot(chromaticDirection, chromaticDirection) < 1.0e-5 &&
        dot(physicalOffset, physicalOffset) > 1.0e-7)
        chromaticDirection = normalize(physicalOffset);
    float chromaticOffset = (pane * (0.00012 + fresnelBand * 0.00006 +
                                     voxelLens * 0.00006) +
                             edge * (0.00080 + coreEdge * 0.00120 +
                                     corner * 0.00100 + fresnelBand * 0.00070)) *
                            refractionStrength;
    float3 chromaticColor;
    chromaticColor.r = sampleSceneColor(
        screenUv + refractionOffset + chromaticDirection * chromaticOffset,
        inputs, rasterOutput, rasterDepthTexture, terrainOutput,
        computeDepthTexture, compositedTexture, sunSprite, cloudOutput).r;
    chromaticColor.g = refractedColor.g;
    chromaticColor.b = sampleSceneColor(
        screenUv + refractionOffset - chromaticDirection * chromaticOffset,
        inputs, rasterOutput, rasterDepthTexture, terrainOutput,
        computeDepthTexture, compositedTexture, sunSprite, cloudOutput).b;

    float refractionMix = pane * clamp(0.095 + fresnelBand * 0.040 +
                                       voxelLens * 0.020, 0.0, 0.19) +
                          edge * clamp(0.58 + coreEdge * 0.16 + corner * 0.12 +
                                       fresnelBand * 0.08, 0.0, 0.92);
    refractionMix = clamp(refractionMix * refractionStrength, 0.0, 0.98);
    float3 glassedColor = lerp(sceneColor, chromaticColor, refractionMix);

    float3 reflectedDirection = reflect(-reflectionViewDirection, reflectionNormal);
    float3 skyReflection = computeSkyWithSunAndStars(
        reflectedDirection, inputs.sun, inputs.camera, inputs.starlight, sunSprite);
    float3 ssrReflection = skyReflection;
    uint outputWidth;
    uint outputHeight;
    compositedTexture.GetDimensions(outputWidth, outputHeight);
    float2 reflectionPixel = screenUv * float2(outputWidth, outputHeight);
    float temporalJitter = float(inputs.environment.frame_serial_idx % 4096u) *
                           0.61803398875;
    float ssrJitterSeed = frac(hash13(float3(reflectionPixel, 17.0)) + temporalJitter);
    float ssrHit = sampleRasterSsrReflection(
        reflectionOriginWorld + reflectedDirection * (TERRARIUM_VOXEL_WORLD_SIZE * 2.0),
        reflectedDirection, ssrJitterSeed, ssrReflection,
        inputs.gui, inputs.camera, rasterOutput, rasterDepthTexture);
    float3 reflectionColor = lerp(skyReflection, ssrReflection,
        clamp(ssrHit * 0.86 * ssrStrength, 0.0, 1.0));
    glassedColor = lerp(glassedColor, ssrReflection,
        clamp(ssrHit * pane * 0.10 * ssrStrength * reflectionStrength, 0.0, 1.0));
    float sunGlint = pow(max(dot(reflectedDirection,
                                 normalize(inputs.sun.sun_dir)), 0.0), 180.0);
    sunGlint *= clamp(inputs.sun.sun_display_luminance, 0.0, 2.0);

    float3 coolTint = float3(0.68, 0.95, 1.00);
    float3 aquaTint = float3(0.50, 1.00, 0.90);
    float3 violetTint = float3(0.88, 0.80, 1.00);
    float3 amberTint = float3(1.00, 0.92, 0.62);
    float3 paneColor = lerp(coolTint, aquaTint, cellNoise * 0.35);
    paneColor = lerp(paneColor, violetTint, cellNoiseB * 0.18);
    paneColor = lerp(paneColor, amberTint, topEdge * 0.22 + cellNoiseC * 0.035);
    float3 sideEdgeColor = float3(0.54, 0.96, 0.92);
    float3 topEdgeColor = float3(1.00, 0.94, 0.74);
    float3 bottomEdgeColor = float3(0.55, 0.74, 1.00);
    float3 edgeColor = lerp(sideEdgeColor, topEdgeColor, topEdge * 0.45);
    edgeColor = lerp(edgeColor, bottomEdgeColor, bottomEdge * 0.35);

    float3 glassColor = lerp(paneColor, edgeColor, edge);
    glassedColor = lerp(glassedColor, glassedColor * configuredTint, 0.18);
    glassColor = lerp(glassColor, configuredTint, 0.28);
    glassColor += reflectionColor * reflectionStrength *
        (edge * 0.14 + coreEdge * 0.08 + fresnelBand * 0.035 +
         pane * (0.018 + reflectionVoxelLens * 0.020 + ssrHit * ssrStrength * 0.070));
    glassColor += ssrReflection * ssrHit * ssrStrength * reflectionStrength *
        (pane * (0.12 + mirrorFresnel * 0.08) + edge * 0.08);
    glassColor += inputs.sun.sun_color * sunGlint * glintStrength *
        (edge * 0.58 + coreEdge * 0.32 + pane * 0.050);

    float paneAlpha = pane * (TERRARIUM_GLASS_ALPHA + fresnelBand * 0.012 +
                              voxelLens * 0.006);
    float edgeAlpha = edge * (0.145 + coreEdge * 0.090 + corner * 0.070 +
                              fresnelBand * 0.030);
    float alpha = clamp((paneAlpha + edgeAlpha) * alphaMultiplier, 0.0, 0.46);
    return lerp(glassedColor, glassColor, alpha);
}

public float3 applyTerrariumGlass(
    float3 sceneColor, float2 screenUv, Ray ray, float sceneDistance,
    TerrariumGlassUniforms inputs,
    Sampler2D rasterOutput, Sampler2D rasterDepthTexture,
    readonly RWTexture2D<float4> terrainOutput,
    readonly RWTexture2D<float> computeDepthTexture,
    RWTexture2D<float4> compositedTexture,
    Sampler2D sunSprite, Sampler2D cloudOutput)
{
    TerrariumGlassHit hits[TERRARIUM_GLASS_MAX_HITS];
    int hitCount = 0;
    float minimumDistance = 0.0;
    for (int index = 0; index < TERRARIUM_GLASS_MAX_HITS; index++)
    {
        TerrariumGlassHit hit = traceTerrariumGlassAfter(
            ray, minimumDistance, inputs.camera);
        if (!hit.hit) break;
        hits[hitCount] = hit;
        hitCount++;
        minimumDistance = hit.t + 0.0001;
    }

    float3 result = sceneColor;
    for (int index = hitCount - 1; index >= 0; index--)
    {
        TerrariumGlassHit hit = hits[index];
        if (sceneDistance + TERRARIUM_GLASS_OCCLUSION_EPS_WORLD < hit.t) continue;
        result = shadeTerrariumGlassHit(
            result, screenUv, ray, hit, inputs, rasterOutput, rasterDepthTexture,
            terrainOutput, computeDepthTexture, compositedTexture,
            sunSprite, cloudOutput);
    }
    return result;
}
