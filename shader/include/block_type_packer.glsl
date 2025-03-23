#ifndef BLOCK_TYPE_AND_WEIGHT_GLSL
#define BLOCK_TYPE_AND_WEIGHT_GLSL

// this file extends support for block types, offering a way to pack a block
// type with the given weight, into a single uint, and unpack it back

#include "../include/block_types.glsl"

// these are adjustable
const float F            = 1.0 / 100.0;
const float BOUNDARY_MIN = -F;
const float BOUNDARY_MAX = F;
const uint N_BITS        = 12;
const uint N_LEVELS      = (1 << N_BITS) - 1;
const uint ENCODED_MASK  = N_LEVELS;

uint _pack_weight(float weight) {
  weight          = clamp(weight, BOUNDARY_MIN, BOUNDARY_MAX);
  const float f01 = (weight - BOUNDARY_MIN) / (BOUNDARY_MAX - BOUNDARY_MIN);
  return uint(f01 * N_LEVELS);
}

float _unpack_weight(uint encoded_weight) {
  return (float(encoded_weight) / float(N_LEVELS)) * (BOUNDARY_MAX - BOUNDARY_MIN) + BOUNDARY_MIN;
}

uint pack_block_type_and_weight(uint block_type, float weight) {
  return (block_type << N_BITS) | _pack_weight(weight);
}

void unpack_block_type_and_weight(out uint o_block_type, out float o_weight, uint data) {
  o_block_type = (data >> N_BITS);
  o_weight     = _unpack_weight(data & ENCODED_MASK);
}

#endif // BLOCK_TYPE_AND_WEIGHT_GLSL
