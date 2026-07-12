#ifndef STYLIZED_VOXEL_LIGHTING_GLSL
#define STYLIZED_VOXEL_LIGHTING_GLSL

// Gives every voxel one stable lighting result instead of shading each cube face separately.
// Callers must provide the shared sun/shading uniforms and direct_sun_shadow.glsl.
float stylized_voxel_direction_weight(vec3 shading_normal) {
    float normal_length_squared = dot(shading_normal, shading_normal);
    if (normal_length_squared <= 1e-8) {
        return 1.0;
    }

    vec3 normal = shading_normal * inversesqrt(normal_length_squared);
    float negative_side = max(0.0, dot(-normal, sun_info.sun_dir));
    return max(0.7, 1.0 - negative_side * negative_side);
}

float stylized_voxel_shadow_weight(vec3 voxel_center, vec3 shading_normal) {
    DirectSunShadowReceiver shadow_receiver = direct_sun_shadow_receiver(
        vec4(voxel_center, 1.0),
        ivec3(0),
        DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_TERRAIN_SHADOW_VSM,
        DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE
    );
    return get_direct_sun_shadow_transmittance(shadow_receiver) *
           stylized_voxel_direction_weight(shading_normal);
}

vec3 apply_stylized_voxel_lighting(vec3 base_color_linear, float shadow_weight) {
    float sun_luminance = sun_luminance_from_dir(sun_info.sun_dir, sun_info.sun_luminance);
    vec3 sun_light = sun_info.sun_color * sun_luminance;
    return base_color_linear * (sun_light * shadow_weight + shading_info.ambient_light);
}

#endif // STYLIZED_VOXEL_LIGHTING_GLSL
