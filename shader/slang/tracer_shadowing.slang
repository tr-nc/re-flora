module tracer_shadowing;

import noise_tex;
import tracer_types;
import terrain_ray_origin;
import vsm;

// Source wiring belongs here, not in each material/gameplay shader. Semantic
// names are resolved by ResourceContainer; these locations are private to this module.
[[vk::binding(80, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(81, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(82, 0)]] Sampler2D leaf_shadow_mask_tex;

static const float TWO_PI = 6.283185307179586;
static const float2 POISSON_16[16] =
{
    float2(0.3040781, -0.1861200), float2(0.1485699, -0.0405212),
    float2(0.4016555, 0.1252352), float2(-0.1526961, -0.1404687),
    float2(0.3480717, 0.3260515), float2(0.0584860, -0.3266001),
    float2(0.0891062, 0.2332856), float2(-0.3487481, -0.0159209),
    float2(-0.1847383, 0.1410431), float2(0.4678784, -0.0888323),
    float2(0.1134236, 0.4119219), float2(0.2856628, -0.3658066),
    float2(-0.1765543, 0.3937907), float2(-0.0238326, 0.0518298),
    float2(-0.2949835, -0.3029899), float2(-0.4593541, 0.1720255)
};

public static const uint DIRECT_SUN_SHADOW_SOURCE_TERRAIN = 1u << 0u;
public static const uint DIRECT_SUN_SHADOW_SOURCE_LEAF = 1u << 1u;
public static const uint DIRECT_SUN_SHADOW_SOURCE_ALL =
    DIRECT_SUN_SHADOW_SOURCE_TERRAIN |
    DIRECT_SUN_SHADOW_SOURCE_LEAF;
public static const uint DIRECT_TERRAIN_SHADOW_VSM = 1u;
public static const uint DIRECT_TERRAIN_SHADOW_PCSS = 2u;
public static const uint DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE = 1u << 0u;
public static const uint DIRECT_SUN_SHADOW_FLAG_RECEIVER_PLANE = 1u << 1u;

struct ShadowReceiverPlane
{
    float2 anchor_uv;
    float anchor_depth;
    float2 depth_gradient_uv;
    bool is_valid;
};

public struct DirectSunShadowReceiver
{
    public float4 terrain_world_position;
    public float4 leaf_world_position;
    public int3 pcss_seed;
    public uint source_mask;
    public uint terrain_mode;
    public uint available_mask;
    public uint flags;
    public float3 receiver_plane_normal;
};

public struct DirectSunShadowTransmittances
{
    public float terrain;
    public float leaf;
    public float combined;
};

public DirectSunShadowReceiver makeDirectSunShadowReceiver(
    float4 terrainWorldPosition, float4 leafWorldPosition,
    int3 pcssSeed, uint sourceMask, uint terrainMode,
    uint availableMask, uint flags)
{
    DirectSunShadowReceiver receiver;
    receiver.terrain_world_position = terrainWorldPosition;
    receiver.leaf_world_position = leafWorldPosition;
    receiver.pcss_seed = pcssSeed;
    receiver.source_mask = sourceMask;
    receiver.terrain_mode = terrainMode;
    receiver.available_mask = availableMask;
    receiver.flags = flags;
    receiver.receiver_plane_normal = float3(0.0);
    return receiver;
}

public DirectSunShadowReceiver withReceiverPlane(
    DirectSunShadowReceiver receiver, float3 normal)
{
    float normalLengthSquared = dot(normal, normal);
    if (normalLengthSquared > 1.0e-8)
    {
        receiver.receiver_plane_normal = normal * rsqrt(normalLengthSquared);
        receiver.flags |= DIRECT_SUN_SHADOW_FLAG_RECEIVER_PLANE;
    }
    return receiver;
}

// Point receivers (soil, models, flora) do not acquire voxel-face offsets.
public DirectSunShadowReceiver pointSunShadowReceiver(
    float3 worldPosition, uint sourceMask, uint availableMask)
{
    float4 position = float4(worldPosition, 1.0);
    return makeDirectSunShadowReceiver(
        position, position, int3(0), sourceMask,
        DIRECT_TERRAIN_SHADOW_VSM, availableMask,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
}

public DirectSunShadowReceiver pointSunShadowReceiver(float3 worldPosition)
{
    return pointSunShadowReceiver(worldPosition,
        DIRECT_SUN_SHADOW_SOURCE_ALL, DIRECT_SUN_SHADOW_SOURCE_ALL);
}

public DirectSunShadowReceiver withLeafReceiver(
    DirectSunShadowReceiver receiver, float3 leafPosition)
{
    receiver.leaf_world_position = float4(leafPosition, 1.0);
    return receiver;
}

// Canonical voxel receivers are stable across pixels. Leaves retain their own
// per-receiver depth gate; never replace the sources with one precombined map.
public DirectSunShadowReceiver voxelSunShadowReceiver(
    float3 voxelCenter, float3 normal, float offsetWorld)
{
    DirectSunShadowReceiver receiver = pointSunShadowReceiver(
        terrainRayOriginAlongNormal(voxelCenter, normal, offsetWorld));
    return withReceiverPlane(receiver, normal);
}

// Exposed faces/triangles already describe a surface, so apply only its normal
// offset. Treating this point as a voxel center would add a second half voxel.
public DirectSunShadowReceiver surfaceSunShadowReceiver(
    float3 surfacePosition, float3 normal, float offsetWorld)
{
    return withReceiverPlane(pointSunShadowReceiver(
        terrainRayOriginFromPosition(surfacePosition, normal, offsetWorld)), normal);
}

float4 sampleExplicitLod(Sampler2D textureSampler, float2 uv)
{
    return textureSampler.SampleLevel(uv, 0.0);
}

float2x2 rotationMatrix(float randomValue)
{
    float angle = randomValue * TWO_PI;
    float cosine = cos(angle);
    float sine = sin(angle);
    return float2x2(cosine, sine, -sine, cosine);
}

ShadowReceiverPlane makeShadowReceiverPlane(
    DirectSunShadowReceiver receiver, float4x4 shadowViewProjectionInverse,
    float2 anchorUv, float anchorDepth)
{
    ShadowReceiverPlane plane;
    plane.anchor_uv = anchorUv;
    plane.anchor_depth = anchorDepth;
    plane.depth_gradient_uv = float2(0.0);
    plane.is_valid = false;
    if ((receiver.flags & DIRECT_SUN_SHADOW_FLAG_RECEIVER_PLANE) == 0u)
        return plane;

    // Directional shadows use an affine orthographic projection. Its plane
    // slope depends only on the transformed normal; the plane constant is
    // supplied exactly by the projected receiver anchor above.
    float4 clipNormal = mul(
        transpose(shadowViewProjectionInverse),
        float4(receiver.receiver_plane_normal, 0.0));
    plane.is_valid = abs(clipNormal.z) > 1.0e-6;
    if (plane.is_valid)
        plane.depth_gradient_uv = -2.0 * clipNormal.xy / clipNormal.z;
    return plane;
}

float receiverDepthAtUv(ShadowReceiverPlane plane, float2 uv,
                        float fallbackDepth)
{
    if (!plane.is_valid) return fallbackDepth;
    return plane.anchor_depth +
        dot(plane.depth_gradient_uv, uv - plane.anchor_uv);
}

float pcf(Sampler2D shadowMap, Sampler2DArray scalarNoise, float2 baseUv,
          float referenceDepth, ShadowReceiverPlane receiverPlane,
          float filterRadius, int3 seed)
{
    // World-space receiver offsets handle acne. Keep only a numerical epsilon
    // here so nearby external casters are not swallowed by a clip-space bias.
    const float shadowEpsilon = 1.0e-5;
    float2x2 rotation = rotationMatrix(randomFloat(scalarNoise, seed));
    float weightedValues = 0.0;
    for (int i = 0; i < 16; ++i)
    {
        float2 offset = mul(rotation, POISSON_16[i]) * filterRadius;
        float2 sampleUv = baseUv + offset;
        float sampleDepth = sampleExplicitLod(shadowMap, sampleUv).r;
        float sampleReferenceDepth = receiverDepthAtUv(
            receiverPlane, sampleUv, referenceDepth);
        weightedValues += sampleDepth + shadowEpsilon > sampleReferenceDepth
            ? 1.0
            : 0.0;
    }
    return weightedValues / 16.0;
}

float averageBlockerSeparation(Sampler2D shadowMap, Sampler2DArray scalarNoise,
                               float2 baseUv, float referenceDepth,
                               ShadowReceiverPlane receiverPlane, int3 seed)
{
    const float shadowEpsilon = 1.0e-5;
    const float blockerSearchWidth = 2.0 / 100.0;
    float2x2 rotation = rotationMatrix(randomFloat(scalarNoise, seed));
    float blockerSeparationSum = 0.0;
    int blockerCount = 0;
    for (int i = 0; i < 16; ++i)
    {
        // Preserve one center lookup so a small contact caster cannot fall
        // between all of the sparse blocker-search taps.
        float2 offset = i == 0
            ? float2(0.0)
            : mul(rotation, POISSON_16[i]) * blockerSearchWidth;
        float2 sampleUv = baseUv + offset;
        float sampleDepth = sampleExplicitLod(shadowMap, sampleUv).r;
        float sampleReferenceDepth = receiverDepthAtUv(
            receiverPlane, sampleUv, referenceDepth);
        if (sampleDepth + shadowEpsilon < sampleReferenceDepth)
        {
            blockerSeparationSum += sampleReferenceDepth - sampleDepth;
            ++blockerCount;
        }
    }
    return blockerCount > 0
        ? blockerSeparationSum / float(blockerCount)
        : -1.0;
}

float2 shadowUvJitter(Sampler2D shadowMap, Sampler2DArray unitVectorNoise, int3 seed)
{
    int3 jitterSeed = int3((seed.xy + int2(17, 53)) % BLUE_NOISE_SIZE.xy,
                           (seed.z + 23) % BLUE_NOISE_SIZE.z);
    uint width;
    uint height;
    uint levels;
    shadowMap.GetDimensions(0, width, height, levels);
    return randomUnitVector2(unitVectorNoise, jitterSeed) * 0.35 / float2(width, height);
}

float pcssTransmittance(Sampler2D shadowMap, Sampler2DArray scalarNoise,
                        Sampler2DArray unitVectorNoise, float4x4 shadowViewProjection,
                        float4x4 shadowViewProjectionInverse,
                        DirectSunShadowReceiver receiver)
{
    float4 lightSpace = mul(
        shadowViewProjection, receiver.terrain_world_position);
    float3 ndc = lightSpace.xyz / lightSpace.w;
    float2 receiverUv = ndc.xy * 0.5 + 0.5;
    ShadowReceiverPlane receiverPlane = makeShadowReceiverPlane(
        receiver, shadowViewProjectionInverse, receiverUv, ndc.z);
    float2 baseUv = receiverUv;
    baseUv += shadowUvJitter(
        shadowMap, unitVectorNoise, receiver.pcss_seed);
    float blockerSeparation = averageBlockerSeparation(
        shadowMap, scalarNoise, baseUv, ndc.z, receiverPlane,
        receiver.pcss_seed);
    if (blockerSeparation < 0.0) return 1.0;
    return pcf(shadowMap, scalarNoise, baseUv, ndc.z, receiverPlane,
               blockerSeparation * (4.0 / 100.0),
               receiver.pcss_seed);
}

float vsmTransmittance(Sampler2D vsmMap, float4x4 shadowViewProjection,
                       float4 worldPosition)
{
    float4 lightSpace = mul(shadowViewProjection, worldPosition);
    float3 ndc = lightSpace.xyz / lightSpace.w;
    float2 uv = ndc.xy * 0.5 + 0.5;
    float2 warpedDepth = warpDepth(ndc.z, evsmExponents());
    float4 occluder = sampleExplicitLod(vsmMap, uv);
    float positive = chebyshevUpperBound(occluder.xz, warpedDepth.x);
    float negative = chebyshevUpperBound(occluder.yw, warpedDepth.y);
    return clamp(min(positive, negative), 0.0, 1.0);
}

// Shared per-tap depth gate. Keeping it separate from texture access lets CPU
// contract tests exercise exactly the policy used by terrain, flora, and soil.
public float leafShadowSampleOpacity(float4 sampleValue, float receiverDepth, bool depthGate)
{
    float opacity = clamp(sampleValue.a, 0.0, 1.0);
    if (depthGate && opacity > 1.0e-4)
    {
        float casterDepth = clamp(sampleValue.r / opacity, 0.0, 1.0);
        if (receiverDepth <= casterDepth + 0.0015) opacity = 0.0;
    }
    return opacity;
}

float leafOpacity(Sampler2D leafOpacityMap, U_GuiInput gui, float2 uv,
                  float receiverDepth, bool depthGate)
{
    uint width;
    uint height;
    uint levels;
    leafOpacityMap.GetDimensions(0, width, height, levels);
    float2 radius = (1.0 / float2(width, height)) *
                    max(gui.leaf_shadow_filter_radius_texels, 0.0);
    float opacity = 0.0;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            float4 sampleValue = sampleExplicitLod(
                leafOpacityMap, uv + float2(float(x), float(y)) * radius);
            float sampleOpacity = leafShadowSampleOpacity(sampleValue, receiverDepth, depthGate);
            opacity = max(opacity, sampleOpacity);
        }
    }
    return clamp(opacity, 0.0, 1.0);
}

float leafTransmittance(Sampler2D leafOpacityMap, Sampler2D leafMaskMap,
                        U_GuiInput gui, float4x4 shadowViewProjection,
                        float4 worldPosition, bool depthGate)
{
    float4 lightSpace = mul(shadowViewProjection, worldPosition);
    float3 ndc = lightSpace.xyz / lightSpace.w;
    float2 uv = ndc.xy * 0.5 + 0.5;
    if (any(uv < float2(0.0)) || any(uv > float2(1.0))) return 1.0;

    float mask = sampleExplicitLod(leafMaskMap, uv).a;
    if (mask < 0.01) return 1.0;
    float opacity = leafOpacity(leafOpacityMap, gui, uv, ndc.z, depthGate) * mask;
    float transmittance = 1.0 - opacity * max(gui.leaf_shadow_strength, 0.0);
    return clamp(transmittance, clamp(gui.leaf_shadow_min_transmittance, 0.0, 1.0), 1.0);
}

bool sourceEnabled(DirectSunShadowReceiver receiver, uint source)
{
    return (receiver.source_mask & source) != 0u &&
           (receiver.available_mask & source) != 0u;
}

DirectSunShadowTransmittances sunShadowWithTerrainVisibility(
    DirectSunShadowReceiver receiver, U_GuiInput gui,
    float4x4 shadowViewProjection, float terrainShadow)
{
    float leafShadow = 1.0;
    if (sourceEnabled(receiver, DIRECT_SUN_SHADOW_SOURCE_LEAF))
    {
        bool depthGate =
            (receiver.flags & DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE) != 0u;
        leafShadow = leafTransmittance(
            leaf_shadow_opacity_blended_tex,
            leaf_shadow_mask_tex,
            gui,
            shadowViewProjection,
            receiver.leaf_world_position,
            depthGate);
    }

    DirectSunShadowTransmittances transmittances;
    transmittances.terrain = terrainShadow;
    transmittances.leaf = leafShadow;
    transmittances.combined = terrainShadow * leafShadow;
    return transmittances;
}

// Stable daylight query used by terrain, raster materials, flora and soil.
public DirectSunShadowTransmittances sampleDirectSunShadow(
    DirectSunShadowReceiver receiver, U_GuiInput gui, U_ShadowCameraInfo shadowCamera)
{
    float terrainShadow = sourceEnabled(receiver, DIRECT_SUN_SHADOW_SOURCE_TERRAIN)
        ? vsmTransmittance(shadow_map_tex_for_vsm_ping,
            shadowCamera.view_proj_mat, receiver.terrain_world_position)
        : 1.0;
    return sunShadowWithTerrainVisibility(
        receiver, gui, shadowCamera.view_proj_mat, terrainShadow);
}

// The PCSS receiver policy remains available without duplicating leaf
// sampling. Its noise and depth dependencies are needed only by PCSS callers.
public float directSunShadowTransmittance(
    DirectSunShadowReceiver receiver, U_GuiInput gui, U_ShadowCameraInfo shadowCamera,
    Sampler2D shadowMap, Sampler2DArray scalarNoise, Sampler2DArray unitVectorNoise)
{
    float terrainShadow = 1.0;
    if (sourceEnabled(receiver, DIRECT_SUN_SHADOW_SOURCE_TERRAIN))
    {
        terrainShadow = receiver.terrain_mode == DIRECT_TERRAIN_SHADOW_VSM
            ? vsmTransmittance(shadow_map_tex_for_vsm_ping,
                shadowCamera.view_proj_mat, receiver.terrain_world_position)
            : pcssTransmittance(shadowMap, scalarNoise, unitVectorNoise,
                shadowCamera.view_proj_mat, shadowCamera.view_proj_mat_inv, receiver);
    }
    return sunShadowWithTerrainVisibility(
        receiver, gui, shadowCamera.view_proj_mat, terrainShadow).combined;
}
