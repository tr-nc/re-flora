#ifndef BUILDER_STRUCTS_GLSL
#define BUILDER_STRUCTS_GLSL

struct PerVoxelBuildInfo {
  uint coordinates;
  uint properties;
};

struct OctreeBuildInfo {
  uint alloc_begin;
  uint alloc_num;
};

#endif // BUILDER_STRUCTS_GLSL
