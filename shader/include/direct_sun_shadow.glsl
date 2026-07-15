#ifndef DIRECT_SUN_SHADOW_GLSL
#define DIRECT_SUN_SHADOW_GLSL

#ifdef DIRECT_SUN_SHADOW_EXPLICIT_LOD
#define DIRECT_SUN_SHADOW_TEXTURE(texture_sampler, uv) textureLod(texture_sampler, uv, 0.0)
#else
#define DIRECT_SUN_SHADOW_TEXTURE(texture_sampler, uv) texture(texture_sampler, uv)
#endif

// Unified direct-sun shadow receiver lookup.
//
// Callers choose which shadow sources are meaningful for the current receiver
// with source_mask and which generated resources are safe to sample with
// available_mask. Unavailable sources are treated as fully lit so callers can
// keep a single getter while scheduling shadow-producing passes explicitly.
//
// Required for terrain VSM:
//   uniform sampler2D shadow_map_tex_for_vsm_ping;
//   U_ShadowCameraInfo shadow_camera_info;
// Required for terrain PCSS when DIRECT_SUN_SHADOW_ENABLE_PCSS is defined:
//   uniform sampler2D shadow_map_tex;
//   blue-noise samplers required by pcss.glsl
// Required for leaf shadows when DIRECT_SUN_SHADOW_ENABLE_LEAF is defined:
//   uniform sampler2D leaf_shadow_opacity_blended_tex;
//   uniform sampler2D leaf_shadow_mask_tex;
//   U_GuiInput gui_input;
// Required for cloud shadows when DIRECT_SUN_SHADOW_ENABLE_CLOUD is defined:
//   uniform sampler2D cloud_shadow_tex;
//   U_GuiInput gui_input;

#include "./vsm.glsl"

#ifdef DIRECT_SUN_SHADOW_ENABLE_PCSS
#include "./pcss.glsl"
#endif

#ifdef DIRECT_SUN_SHADOW_ENABLE_LEAF
#include "./leaf_shadow.glsl"
#endif

#ifdef DIRECT_SUN_SHADOW_ENABLE_CLOUD
#include "./cloud_shadow.glsl"
#endif

const uint DIRECT_SUN_SHADOW_SOURCE_TERRAIN = 1u << 0u;
const uint DIRECT_SUN_SHADOW_SOURCE_LEAF    = 1u << 1u;
const uint DIRECT_SUN_SHADOW_SOURCE_CLOUD   = 1u << 2u;
const uint DIRECT_SUN_SHADOW_SOURCE_ALL = DIRECT_SUN_SHADOW_SOURCE_TERRAIN |
                                         DIRECT_SUN_SHADOW_SOURCE_LEAF |
                                         DIRECT_SUN_SHADOW_SOURCE_CLOUD;

const uint DIRECT_TERRAIN_SHADOW_NONE = 0u;
const uint DIRECT_TERRAIN_SHADOW_VSM  = 1u;
const uint DIRECT_TERRAIN_SHADOW_PCSS = 2u;
const uint DIRECT_TERRAIN_SHADOW_GUI  = 3u;

const uint DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE = 1u << 0u;

struct DirectSunShadowReceiver {
    vec4 world_pos;
    ivec3 pcss_seed;
    uint source_mask;
    uint terrain_mode;
    uint available_mask;
    uint flags;
};

DirectSunShadowReceiver direct_sun_shadow_receiver(vec4 world_pos, ivec3 pcss_seed,
                                                   uint source_mask, uint terrain_mode,
                                                   uint available_mask, uint flags) {
    DirectSunShadowReceiver receiver;
    receiver.world_pos = world_pos;
    receiver.pcss_seed = pcss_seed;
    receiver.source_mask = source_mask;
    receiver.terrain_mode = terrain_mode;
    receiver.available_mask = available_mask;
    receiver.flags = flags;
    return receiver;
}

bool direct_sun_shadow_source_enabled(DirectSunShadowReceiver receiver, uint source) {
    return (receiver.source_mask & source) != 0u && (receiver.available_mask & source) != 0u;
}

float get_direct_sun_terrain_shadow_transmittance(DirectSunShadowReceiver receiver) {
    if (!direct_sun_shadow_source_enabled(receiver, DIRECT_SUN_SHADOW_SOURCE_TERRAIN)) {
        return 1.0;
    }

    uint terrain_mode = receiver.terrain_mode;
    if (terrain_mode == DIRECT_TERRAIN_SHADOW_GUI) {
#ifdef DIRECT_SUN_SHADOW_ENABLE_PCSS
        terrain_mode = gui_input.terrain_shadow_use_vsm != 0u ? DIRECT_TERRAIN_SHADOW_VSM
                                                              : DIRECT_TERRAIN_SHADOW_PCSS;
#else
        terrain_mode = DIRECT_TERRAIN_SHADOW_VSM;
#endif
    }

    if (terrain_mode == DIRECT_TERRAIN_SHADOW_VSM) {
        return clamp(get_shadow_weight_vsm_from_map(shadow_map_tex_for_vsm_ping,
                                                    shadow_camera_info.view_proj_mat,
                                                    receiver.world_pos),
                     0.0, 1.0);
    }

#ifdef DIRECT_SUN_SHADOW_ENABLE_PCSS
    if (terrain_mode == DIRECT_TERRAIN_SHADOW_PCSS) {
        return clamp(get_shadow_weight_pcss(receiver.world_pos.xyz, receiver.pcss_seed), 0.0, 1.0);
    }
#endif

    return 1.0;
}

float get_direct_sun_leaf_shadow_transmittance(DirectSunShadowReceiver receiver) {
#ifndef DIRECT_SUN_SHADOW_ENABLE_LEAF
    return 1.0;
#else
    if (!direct_sun_shadow_source_enabled(receiver, DIRECT_SUN_SHADOW_SOURCE_LEAF)) {
        return 1.0;
    }

    bool depth_gate = (receiver.flags & DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE) != 0u;
    return get_leaf_shadow_transmittance(receiver.world_pos, true, depth_gate);
#endif
}

float get_direct_sun_cloud_shadow_transmittance(DirectSunShadowReceiver receiver) {
#ifndef DIRECT_SUN_SHADOW_ENABLE_CLOUD
    return 1.0;
#else
    if (!direct_sun_shadow_source_enabled(receiver, DIRECT_SUN_SHADOW_SOURCE_CLOUD)) {
        return 1.0;
    }

    return get_cloud_shadow_transmittance(receiver.world_pos);
#endif
}

float get_direct_sun_shadow_transmittance(DirectSunShadowReceiver receiver) {
    float terrain_shadow = get_direct_sun_terrain_shadow_transmittance(receiver);
    float leaf_shadow = get_direct_sun_leaf_shadow_transmittance(receiver);
    float cloud_shadow = get_direct_sun_cloud_shadow_transmittance(receiver);
    return terrain_shadow * leaf_shadow * cloud_shadow;
}

#endif // DIRECT_SUN_SHADOW_GLSL
