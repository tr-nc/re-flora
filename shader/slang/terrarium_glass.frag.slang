import tracer_types;

[[vk::binding(1, 0)]]
ConstantBuffer<U_SunInfo> sun_info;

static const float TERRARIUM_VOXELS_PER_CHUNK_AXIS = 256.0;
static const float TERRARIUM_VOXEL_WORLD_SIZE =
    1.0 / TERRARIUM_VOXELS_PER_CHUNK_AXIS;
static const float TERRARIUM_GLASS_THICKNESS_VOXELS = 2.0;
static const float TERRARIUM_GLASS_THICKNESS_WORLD =
    TERRARIUM_GLASS_THICKNESS_VOXELS * TERRARIUM_VOXEL_WORLD_SIZE;
static const float TERRARIUM_GLASS_CORE_THICKNESS_WORLD =
    TERRARIUM_GLASS_THICKNESS_WORLD / 3.0;
static const float TERRARIUM_GLASS_TOP_PADDING_WORLD = 0.08;
static const float TERRARIUM_GLASS_FACE_WORLD_WIDTH =
    1.0 + TERRARIUM_GLASS_THICKNESS_WORLD * 2.0;
static const float TERRARIUM_GLASS_FACE_WORLD_HEIGHT =
    1.0 + TERRARIUM_GLASS_TOP_PADDING_WORLD;

struct FragmentInput
{
    [[vk::location(0)]] float3 position_ws : TEXCOORD0;
    [[vk::location(1)]] float2 uv : TEXCOORD1;
    [[vk::location(2)]] nointerpolation uint face_id : TEXCOORD2;
    [[vk::location(3)]] nointerpolation float near_side : TEXCOORD3;
    [[vk::location(4)]] nointerpolation float2 alpha_pair : TEXCOORD4;
    [[vk::location(5)]] float3 normal_ws : NORMAL0;
    [[vk::location(6)]] nointerpolation uint part_kind : TEXCOORD5;
    [[vk::location(7)]] float3 view_direction_ws : TEXCOORD6;
};

float3 skyReflectionColor(float3 direction)
{
    float up = clamp(direction.y * 0.5 + 0.5, 0.0, 1.0);
    float3 lowSky = float3(0.46, 0.68, 0.86);
    float3 highSky = float3(0.88, 0.97, 1.00);
    float3 groundGreen = float3(0.12, 0.23, 0.17);
    float3 sky = lerp(lowSky, highSky, up * up);
    return direction.y >= 0.0
        ? sky
        : lerp(lowSky, groundGreen, clamp(-direction.y, 0.0, 1.0));
}

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    bool invalidInput =
        input.face_id == 0xffffffffu || any(isnan(input.position_ws));
    float2 uv = clamp(input.uv, float2(0.0), float2(1.0));
    float2 edgeWidth = float2(
        TERRARIUM_GLASS_THICKNESS_WORLD /
            TERRARIUM_GLASS_FACE_WORLD_WIDTH,
        TERRARIUM_GLASS_THICKNESS_WORLD /
            TERRARIUM_GLASS_FACE_WORLD_HEIGHT);
    float2 coreWidth = float2(
        TERRARIUM_GLASS_CORE_THICKNESS_WORLD /
            TERRARIUM_GLASS_FACE_WORLD_WIDTH,
        TERRARIUM_GLASS_CORE_THICKNESS_WORLD /
            TERRARIUM_GLASS_FACE_WORLD_HEIGHT);
    float sideEdge = max(
        1.0 - step(edgeWidth.x, uv.x),
        step(1.0 - edgeWidth.x, uv.x));
    float topEdge = step(1.0 - edgeWidth.y, uv.y);
    float bottomEdge = 1.0 - step(edgeWidth.y, uv.y);
    float uvEdge = max(sideEdge, max(topEdge, bottomEdge));
    float coreSideEdge = max(
        1.0 - step(coreWidth.x, uv.x),
        step(1.0 - coreWidth.x, uv.x));
    float coreTopEdge = step(1.0 - coreWidth.y, uv.y);
    float coreBottomEdge = 1.0 - step(coreWidth.y, uv.y);
    float coreEdge = max(coreSideEdge, max(coreTopEdge, coreBottomEdge));
    float corner = sideEdge * max(topEdge, bottomEdge);

    float3 viewDirection = normalize(input.view_direction_ws);
    float3 normal = normalize(input.normal_ws);
    normal = dot(normal, viewDirection) < 0.0 ? -normal : normal;
    float normalDotView = clamp(dot(normal, viewDirection), 0.0, 1.0);
    float fresnel = step(0.42, pow(1.0 - normalDotView, 4.0));
    float3 reflectedDirection = reflect(-viewDirection, normal);
    float3 environmentReflection = skyReflectionColor(reflectedDirection);

    float3 sunDirection = normalize(sun_info.sun_dir);
    float sunGlint = pow(
        max(dot(reflectedDirection, sunDirection), 0.0), 96.0);
    sunGlint *= clamp(sun_info.sun_display_luminance, 0.0, 2.0);

    float nearSide = clamp(input.near_side, 0.0, 1.0);
    float bodyAlpha = lerp(input.alpha_pair.y, input.alpha_pair.x, nearSide);
    float paneAlpha = bodyAlpha + fresnel * 0.030 + uvEdge * 0.016;
    float edgeAlpha = 0.22 + fresnel * 0.24 +
                      coreEdge * 0.18 + corner * 0.12;
    float rimAlpha = 0.28 + fresnel * 0.22 + coreEdge * 0.16;
    float cornerAlpha = 0.45 + fresnel * 0.26 + coreEdge * 0.20;

    float alpha = paneAlpha;
    if (input.part_kind == 1u)
    {
        alpha = edgeAlpha;
    }
    else if (input.part_kind == 2u)
    {
        alpha = rimAlpha;
    }
    else if (input.part_kind == 3u)
    {
        alpha = cornerAlpha;
    }
    alpha = clamp(alpha, 0.0, 0.78);

    float3 paneColor = lerp(
        float3(0.62, 0.82, 0.94),
        float3(0.86, 0.96, 1.00),
        nearSide);
    float3 edgeColor = float3(0.70, 1.00, 0.96);
    float3 topColor = float3(1.00, 0.94, 0.74);
    float3 bottomColor = float3(0.54, 0.76, 1.00);
    float3 cornerColor = float3(0.96, 1.00, 0.98);

    float3 color = paneColor;
    float edgeMaterial = input.part_kind == 0u ? 0.0 : 1.0;
    color = lerp(color, edgeColor, edgeMaterial * 0.55);
    color = lerp(
        color,
        bottomColor,
        max(bottomEdge, step(0.6, -normal.y)) * edgeMaterial * 0.35);
    color = lerp(
        color,
        topColor,
        max(topEdge, step(0.6, normal.y)) * edgeMaterial * 0.40);
    color = lerp(
        color,
        cornerColor,
        max(corner, input.part_kind == 3u ? 1.0 : 0.0) * 0.42);

    float longTopGlint = 0.0;
    if (input.part_kind == 2u && normal.y > 0.35)
    {
        longTopGlint = step(0.08, uv.x) * (1.0 - step(0.96, uv.x));
    }

    float opticalEdge = max(edgeMaterial, uvEdge * 0.35);
    float3 reflection = environmentReflection *
        (0.08 + fresnel * 0.45 + opticalEdge * 0.26);
    reflection += sun_info.sun_color *
        (sunGlint * (0.20 + opticalEdge * 0.90));

    float3 spectral = float3(0.10, 0.02, 0.00) * topEdge;
    spectral += float3(0.00, 0.12, 0.18) * sideEdge;
    spectral += float3(0.00, 0.05, 0.14) * bottomEdge;
    spectral *= opticalEdge * (0.35 + fresnel);

    color += reflection + spectral;
    color = lerp(
        color,
        float3(1.18, 1.28, 1.22),
        coreEdge * edgeMaterial * 0.42);
    color += float3(1.25, 1.14, 0.82) * longTopGlint * 0.38;
    return invalidInput
        ? float4(0.0)
        : float4(color * alpha, alpha);
}
