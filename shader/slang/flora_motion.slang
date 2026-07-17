module flora_motion;

import flora_noise;
import flora_types;
import hash;
import tracer_types;

static const float FLORA_TWO_PI = 6.28318530718;
static const float FLORA_NATURAL_BEND_MAX_ANGLE = 1.45;
static const uint WIND_VOLUME_BUCKET_COUNT = 4u;

public float floraSmootherstep(float value)
{
    value = clamp(value, 0.0, 1.0);
    return value * value * value *
           (value * (value * 6.0 - 15.0) + 10.0);
}

float floraBendHeightFactor(U_GuiInput gui, float heightFraction)
{
    return pow(clamp(heightFraction, 0.0, 1.0),
               max(gui.flora_bend_height_power, 0.05));
}

float floraWindPlanarStrength(float3 wind)
{
    return smoothstep(0.03, 2.0, length(wind.xz));
}

float2 floraWindPlanarDirection(float3 wind)
{
    float2 planar = wind.xz;
    float planarLength = length(planar);
    return planarLength > 1.0e-4 ? planar / planarLength : float2(1.0, 0.0);
}

float floraWindCurveResponse(float windStrength, float startStrength,
                             float fullStrength, float kneeBias)
{
    float low = min(startStrength, fullStrength);
    float high = max(startStrength, fullStrength);
    float range = high - low;
    float value = range <= 1.0e-4
        ? (windStrength >= high ? 1.0 : 0.0)
        : clamp((windStrength - low) / range, 0.0, 1.0);
    float exponent = exp2(clamp(kneeBias, -6.0, 6.0));
    return floraSmootherstep(pow(value, exponent));
}

float floraWindPhase(uint seed, int3 voxelLocalPosition, uint salt)
{
    uint value = seed ^ salt;
    value ^= uint(voxelLocalPosition.x) * 0x85EBCA6Bu;
    value ^= uint(voxelLocalPosition.y) * 0xC2B2AE35u;
    value ^= uint(voxelLocalPosition.z) * 0x27D4EB2Fu;
    return constructFloat01(wellonsHash(value)) * FLORA_TWO_PI;
}

public uint windVolumeBucketIndex(uint seed)
{
    return seed % WIND_VOLUME_BUCKET_COUNT;
}

public uint windVolumeVoxelSeed(uint seed, int3 voxelLocalPosition)
{
    uint value = seed ^ 0x9E3779B9u;
    value ^= uint(voxelLocalPosition.x) * 0x85EBCA6Bu;
    value ^= uint(voxelLocalPosition.y) * 0xC2B2AE35u;
    value ^= uint(voxelLocalPosition.z) * 0x27D4EB2Fu;
    return value ^ (value >> 16u);
}

public float windVolumeBucketUpdateTime(U_GuiInput gui, uint bucketIndex,
                                        float time)
{
    float stepSeconds = gui.world_tick_seconds;
    if (stepSeconds <= 0.0) return time;

    uint stepIndex = uint(floor(max(time, 0.0) / stepSeconds));
    if (stepIndex < bucketIndex) return 0.0;

    uint lastBucketStep = bucketIndex +
        ((stepIndex - bucketIndex) / WIND_VOLUME_BUCKET_COUNT) * WIND_VOLUME_BUCKET_COUNT;
    return float(lastBucketStep) * stepSeconds;
}

public float3 sampleWindVolume(U_WindVolumeInfo windInfo, Sampler3D windTexture,
                               float3 worldPosition, uint seed)
{
    float3 localUv = clamp(worldPosition / windInfo.world_chunk_extent, 0.0, 1.0);
    uint width;
    uint height;
    uint depth;
    uint levels;
    windTexture.GetDimensions(0, width, height, depth, levels);
    float bucketWidth = float(width) / float(WIND_VOLUME_BUCKET_COUNT);
    float bucketX = float(windVolumeBucketIndex(seed)) * bucketWidth +
                    localUv.x * (bucketWidth - 1.0) + 0.5;
    float sampleY = localUv.y * float(height - 1u) + 0.5;
    float sampleZ = localUv.z * float(depth - 1u) + 0.5;
    float3 windUv = float3(bucketX / float(width), sampleY / float(height),
                           sampleZ / float(depth));
    float2 planar = windTexture.SampleLevel(windUv, 0.0).xy;
    return float3(planar.x, 0.0, planar.y);
}

public uint floraSpeciesWindMotionTopology(uint instanceType)
{
    if (instanceType == FLORA_SPECIES_KOCHIA) return 1u;
    if (instanceType == FLORA_SPECIES_TALL_GRASS ||
        instanceType == FLORA_SPECIES_SHORT_GRASS ||
        instanceType == FLORA_SPECIES_APPLE) return 0u;
    return 2u;
}

public uint floraGroupedStemWindSeed(uint seed, uint animationGroup)
{
    return seed + (animationGroup > 0u ? animationGroup - 1u : 0u);
}

public float floraSpeciesVoxelWindAffectMultiplier(U_GuiInput gui,
                                                    uint instanceType)
{
    return instanceType == FLORA_SPECIES_KOCHIA
        ? max(gui.kochia_body_wind_response, 0.0)
        : 1.0;
}

public float3 floraNaturalRestBend(U_GuiInput gui, uint seed,
                                   float heightFraction, float floraHeightVoxels)
{
    float directionAngle =
        constructFloat01(wellonsHash(seed ^ 0xD1B54A32u)) * FLORA_TWO_PI;
    float3 bendDirection = float3(cos(directionAngle), 0.0, sin(directionAngle));

    float amountJitter = constructFloat01(wellonsHash(seed ^ 0x94D049BBu));
    float bendMinimum = max(gui.grass_natural_bend_min_voxels, 0.0);
    float bendMaximum = max(gui.grass_natural_bend_max_voxels, bendMinimum);
    float tipBendVoxels = lerp(bendMinimum, bendMaximum, amountJitter);

    float bladeHeight = max(floraHeightVoxels, 1.0);
    float bendValue = floraBendHeightFactor(gui, clamp(heightFraction, 0.0, 1.0));
    float bendCenterY = bendValue * bladeHeight;
    float bendAngle = clamp(2.0 * tipBendVoxels / bladeHeight, 0.0,
                            FLORA_NATURAL_BEND_MAX_ANGLE);
    if (bendAngle <= 1.0e-4) return float3(0.0);

    float radius = bladeHeight / bendAngle;
    float arcAngle = bendAngle * bendValue;
    float horizontalOffset = radius * (1.0 - cos(arcAngle));
    float verticalOffset = radius * sin(arcAngle) - bendCenterY;
    return bendDirection * horizontalOffset + float3(0.0, verticalOffset, 0.0);
}

public float3 floraWindVibration(U_GuiInput gui, float3 wind, float windGradient,
                                 uint seed, int3 voxelLocalPosition, float time)
{
    float strength = floraWindPlanarStrength(wind);
    if (strength <= 0.0) return float3(0.0);

    float2 windDirection = floraWindPlanarDirection(wind);
    float3 crossWindDirection = float3(-windDirection.y, 0.0, windDirection.x);
    float tipWeight = floraBendHeightFactor(gui, windGradient);
    float phase = floraWindPhase(seed, voxelLocalPosition, 0xB5297A4Du);
    float heightPhase = float(voxelLocalPosition.y) * 1.35;
    float vibration = sin(time * gui.grass_vibration_primary_speed + phase + heightPhase);
    vibration += 0.45 * sin(time * gui.grass_vibration_secondary_speed +
                            phase * 1.37 - heightPhase * 0.6);
    return crossWindDirection *
           (vibration * gui.grass_vibration_amplitude_voxels * strength * tipWeight);
}

public float3 kochiaBranchJellyMotion(U_GuiInput gui, float3 wind,
                                      float branchHeight, uint seed,
                                      uint animationGroup, float time)
{
    float strength = floraWindPlanarStrength(wind);
    float branchWeight = floraSmootherstep(branchHeight);
    if (strength <= 0.0 || branchWeight <= 0.0) return float3(0.0);

    float2 windDirection = floraWindPlanarDirection(wind);
    float3 alongWindDirection = float3(windDirection.x, 0.0, windDirection.y);
    float3 crossWindDirection = float3(-windDirection.y, 0.0, windDirection.x);
    uint groupSeed = wellonsHash(seed ^ (animationGroup * 0x9E3779B9u));
    float instancePhase = constructFloat01(wellonsHash(seed ^ 0xA511E9B3u));
    float groupPhase = frac(instancePhase + float(animationGroup) * 0.61803398875 *
                                             gui.kochia_branch_phase_spread) *
                       FLORA_TWO_PI;
    float directionJitter = constructFloat01(groupSeed) * 2.0 - 1.0;
    float3 branchDirection = normalize(
        alongWindDirection + crossWindDirection * directionJitter * 0.7);
    float speedJitter = lerp(0.85, 1.15,
        constructFloat01(wellonsHash(groupSeed ^ 0xB5297A4Du)));
    float jellySpeed = max(gui.kochia_branch_jelly_speed, 0.0) * speedJitter;
    float jelly = sin(time * jellySpeed + groupPhase);
    jelly += 0.35 * sin(time * jellySpeed * 1.67 + groupPhase * 1.31);
    jelly *= 1.0 / 1.35;

    float3 offset = branchDirection *
        (jelly * gui.kochia_branch_jelly_amplitude_voxels * strength * branchWeight);
    float flutter = sin(time * max(gui.kochia_tip_flutter_speed, 0.0) +
                        groupPhase * 1.73 + float(animationGroup) * 0.41);
    float tipWeight = branchWeight * branchWeight * branchWeight;
    return offset + crossWindDirection *
        (flutter * gui.kochia_tip_flutter_amplitude_voxels * strength * tipWeight);
}

public float3 appleWindSwing(U_GuiInput gui, float3 wind, uint seed, float time)
{
    float strength = max(clamp(gui.fruit_swing_min_response, 0.0, 1.0),
                         floraWindPlanarStrength(wind));
    float2 windDirection = floraWindPlanarDirection(wind);
    float3 alongWindDirection = float3(windDirection.x, 0.0, windDirection.y);
    float3 crossWindDirection = float3(-windDirection.y, 0.0, windDirection.x);
    float phase = constructFloat01(wellonsHash(seed ^ 0x9E3779B9u)) * FLORA_TWO_PI;
    float speedJitter = constructFloat01(wellonsHash(seed ^ 0xB7E15162u)) * 2.0 - 1.0;
    float directionJitter =
        constructFloat01(wellonsHash(seed ^ 0xC2B2AE35u)) * 2.0 - 1.0;
    float speed = max(gui.fruit_swing_speed, 0.0) *
                  max(0.0, 1.0 + speedJitter * gui.fruit_swing_speed_variation);
    float3 swingDirection = normalize(
        alongWindDirection + crossWindDirection * directionJitter * 0.35);
    float angle = sin(time * speed + phase) *
                  max(gui.fruit_swing_max_angle_radians, 0.0) * strength;
    float pivotLength = max(gui.fruit_swing_length_voxels, 0.0);
    return swingDirection * (sin(angle) * pivotLength) +
           float3(0.0, (1.0 - cos(angle)) * pivotLength, 0.0);
}

public float3 leafWindPaddling(U_GuiInput gui, float3 wind, float windGradient,
                               uint seed, int3 voxelLocalPosition, float time)
{
    float windStrength = length(wind.xz);
    float amplitudeResponse = floraWindCurveResponse(
        windStrength, gui.leaf_paddle_amplitude_wind_start_strength,
        gui.leaf_paddle_amplitude_wind_full_strength,
        gui.leaf_paddle_amplitude_wind_knee_bias);
    if (amplitudeResponse <= 0.0) return float3(0.0);

    float frequencyResponse = floraWindCurveResponse(
        windStrength, gui.leaf_paddle_frequency_wind_start_strength,
        gui.leaf_paddle_frequency_wind_full_strength,
        gui.leaf_paddle_frequency_wind_knee_bias);
    float frequencyMultiplier = max(0.0, lerp(
        gui.leaf_paddle_frequency_min_multiplier,
        gui.leaf_paddle_frequency_max_multiplier, frequencyResponse));

    float2 windDirection = floraWindPlanarDirection(wind);
    float3 crossWindDirection = float3(-windDirection.y, 0.0, windDirection.x);
    float3 flapDirection = normalize(float3(
        crossWindDirection.x * 0.35, 1.0, crossWindDirection.z * 0.35));
    float3 localDirection = normalize(float3(voxelLocalPosition) + 1.0e-3);
    float windSide = dot(localDirection,
                         float3(windDirection.x, 0.0, windDirection.y));
    float phase = floraWindPhase(seed, voxelLocalPosition, 0x68E31DA4u);
    float shellPhase = windSide * 2.4 + float(voxelLocalPosition.y) * 0.13;
    float paddle = sin(time * gui.leaf_paddle_primary_speed * frequencyMultiplier +
                       phase + shellPhase);
    paddle += 0.35 * sin(time * gui.leaf_paddle_secondary_speed * frequencyMultiplier +
                         phase * 1.61 - shellPhase * 0.7);
    float shellWeight = pow(clamp(windGradient, 0.0, 1.0), 1.15);
    float exposedSideWeight = 0.55 + 0.45 * abs(windSide);
    return flapDirection *
        (paddle * gui.leaf_paddle_amplitude_voxels * amplitudeResponse *
         shellWeight * exposedSideWeight);
}

public FloraSpawnAnimationPose sampleFloraSpawnAnimation(
    U_FloraGrowthInfo growthInfo, uint spawnStartMs, uint seed,
    float plantHeightVoxels)
{
    FloraSpawnAnimationPose pose;
    pose.translation_voxels = float3(0.0);
    pose.scale = float3(1.0);
    pose.visibility = 1.0;
    if (spawnStartMs == INSTANCE_SPAWN_INACTIVE) return pose;

    float duration = max(growthInfo.spawn_duration_seconds, 0.001);
    float stagger = constructFloat01(wellonsHash(seed ^ 0xD1B54A35u)) *
                    max(growthInfo.spawn_stagger_seconds, 0.0);
    float ageSeconds = float(instanceSpawnAgeMs(
        spawnStartMs, growthInfo.spawn_time_ms)) * 0.001 - stagger;
    float timeline = clamp(ageSeconds / duration, 0.0, 1.0);
    float riseEnd = clamp(growthInfo.spawn_rise_fraction, 0.05, 0.95);
    float overshootMinimum = min(growthInfo.spawn_overshoot_min_voxels,
                                 growthInfo.spawn_overshoot_max_voxels);
    float overshootMaximum = max(growthInfo.spawn_overshoot_min_voxels,
                                 growthInfo.spawn_overshoot_max_voxels);
    float overshoot = clamp(plantHeightVoxels * 0.08,
                            overshootMinimum, overshootMaximum);
    float underground = -(plantHeightVoxels + 1.0);

    float verticalOffset;
    if (timeline < riseEnd)
    {
        float value = clamp(timeline / riseEnd, 0.0, 1.0);
        float inverse = 1.0 - value;
        verticalOffset = lerp(underground, overshoot,
                              1.0 - inverse * inverse * inverse);
    }
    else
    {
        float value = clamp((timeline - riseEnd) / (1.0 - riseEnd), 0.0, 1.0);
        value = value * value * (3.0 - 2.0 * value);
        verticalOffset = lerp(overshoot, 0.0, value);
    }
    pose.translation_voxels.y = verticalOffset;
    return pose;
}
