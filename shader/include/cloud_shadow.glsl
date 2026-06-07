#ifndef CLOUD_SHADOW_GLSL
#define CLOUD_SHADOW_GLSL

// Cheap cloud shadow receiver lookup.
// Requires:
//   uniform sampler2D cloud_shadow_tex;
//   U_ShadowCameraInfo shadow_camera_info;
//   U_GuiInput gui_input;

float get_cloud_shadow_transmittance(vec4 voxel_pos_ws) {
    if (gui_input.cloud_shadows_enabled == 0u || gui_input.clouds_enabled == 0u) {
        return 1.0;
    }

    // The generated shadow texture represents cloud transmittance for receivers
    // below the cloud layer. Avoid darkening high objects above the layer with a
    // column shadow that should have passed underneath them.
    if (voxel_pos_ws.y > gui_input.cloud_top_height) {
        return 1.0;
    }

    vec4 light_space = shadow_camera_info.view_proj_mat * voxel_pos_ws;
    vec3 ndc = light_space.xyz / light_space.w;
    vec2 uv = ndc.xy * 0.5 + 0.5;

    if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
        return 1.0;
    }

    return clamp(texture(cloud_shadow_tex, uv).r, 0.0, 1.0);
}

#endif // CLOUD_SHADOW_GLSL
