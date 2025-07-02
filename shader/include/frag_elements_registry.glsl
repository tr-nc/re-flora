#ifndef FRAG_ELEMENTS_REGISTRY_GLSL
#define FRAG_ELEMENTS_REGISTRY_GLSL

struct GrassInstance {
    uvec3 position;
    uint grass_type;
};

struct LeafInstance {
    uvec3 position;
    vec3 base_color;
};

#endif // FRAG_ELEMENTS_REGISTRY_GLSL
