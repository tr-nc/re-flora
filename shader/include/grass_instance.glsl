#ifndef GRASS_INSTANCE_GLSL
#define GRASS_INSTANCE_GLSL

struct GrassInstance {
    uvec3 position; // TODO: maybe use lower memory footprint
    uint grass_type;
};

#endif // GRASS_INSTANCE_GLSL
