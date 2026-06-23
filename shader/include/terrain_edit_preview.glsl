#ifndef TERRAIN_EDIT_PREVIEW_GLSL
#define TERRAIN_EDIT_PREVIEW_GLSL

#ifndef TERRAIN_EDIT_PREVIEW_DESCRIPTOR_SET
#define TERRAIN_EDIT_PREVIEW_DESCRIPTOR_SET 0
#endif

#ifndef TERRAIN_EDIT_PREVIEW_BINDING
#define TERRAIN_EDIT_PREVIEW_BINDING 15
#endif

layout(set = TERRAIN_EDIT_PREVIEW_DESCRIPTOR_SET, binding = TERRAIN_EDIT_PREVIEW_BINDING) uniform
    U_TerrainEditPreview {
    vec3 center;
    float radius;
    vec3 color;
    float strength;
    uint enabled;
    uint shape;
}
terrain_edit_preview;

float terrain_edit_preview_tint_at(vec3 center_pos) {
    if (terrain_edit_preview.enabled == 0u || terrain_edit_preview.radius <= 0.0) {
        return 0.0;
    }

    float dist = terrain_edit_preview.shape == 1u
                     ? length(center_pos.xz - terrain_edit_preview.center.xz)
                     : length(center_pos - terrain_edit_preview.center);
    float edge = max(terrain_edit_preview.radius * 0.12, 0.002);
    float mask = 1.0 - smoothstep(terrain_edit_preview.radius - edge,
                                   terrain_edit_preview.radius, dist);
    return clamp(mask * terrain_edit_preview.strength, 0.0, 1.0);
}

vec3 terrain_edit_preview_overlay_blend(vec3 base_color, vec3 overlay_color) {
    vec3 low = 2.0 * base_color * overlay_color;
    vec3 high = 1.0 - 2.0 * (1.0 - base_color) * (1.0 - overlay_color);
    return mix(low, high, step(vec3(0.5), base_color));
}

vec3 apply_terrain_edit_preview_tint(vec3 base_color, vec3 center_pos) {
    float terrain_edit_tint = terrain_edit_preview_tint_at(center_pos);
    if (terrain_edit_tint <= 0.0) {
        return base_color;
    }

    vec3 overlay_color = terrain_edit_preview_overlay_blend(base_color, terrain_edit_preview.color);
    return mix(base_color, overlay_color, terrain_edit_tint);
}

#endif // TERRAIN_EDIT_PREVIEW_GLSL
