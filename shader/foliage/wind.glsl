#ifndef WIND_GLSL
#define WIND_GLSL

vec2 rand_offset(vec2 instance_pos, float time) {
    const float wind_speed            = 0.6;
    const float wind_strength         = 5.0;
    const float wind_scale            = 2.0;
    const float natual_variance_scale = 1.5;

    // ranges from 0 to 1
    vec2 natual_state = hash_22(instance_pos);
    // convert to -1 to 1
    natual_state = natual_state * 2.0 - 1.0;
    natual_state = natual_state * natual_variance_scale;

    fnl_state state    = fnlCreateState(469);
    state.noise_type   = FNL_NOISE_PERLIN;
    state.fractal_type = FNL_FRACTAL_FBM;
    state.frequency    = wind_scale;
    state.octaves      = 2;
    state.lacunarity   = 2.0;
    state.gain         = 0.2;

    float time_offset = time * wind_speed;

    float noise_x = fnlGetNoise2D(state, instance_pos.x + time_offset, instance_pos.y);

    // Sample noise for the Z offset from a different location in the noise field to make it look
    // more natural. Adding a large number to the coordinates ensures we are sampling a different,
    // uncorrelated noise pattern.
    float noise_z =
        fnlGetNoise2D(state, instance_pos.x + 123.4, instance_pos.y - 234.5 + time_offset);

    // The noise is in the range [-1, 1], we scale it by the desired strength.
    return vec2(noise_x, noise_z) * wind_strength + natual_state;
}

vec3 get_wind_offset(vec2 instance_pos, float gradient, float time) {
    vec2 rand_off = rand_offset(instance_pos, time) * gradient * gradient;
    return vec3(rand_off.x, 0.0, rand_off.y);
}

#endif // WIND_GLSL
