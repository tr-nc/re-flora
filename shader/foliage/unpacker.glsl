#ifndef UNPACKER_GLSL
#define UNPACKER_GLSL

const uint BIT_PER_POS    = 7;
const uint BIT_PER_OFFSET = 1;

const uint BIT_PER_CUSTOM_INFO    = 8;
const uint BIT_PER_COLOR_GRADIENT = 4;
const uint BIT_PER_WIND_GRADIENT  = 4;

const uint POS_BITS            = BIT_PER_POS * 3;
const uint OFFSET_BITS         = BIT_PER_OFFSET * 3;
const uint POS_MASK            = (1u << BIT_PER_POS) - 1u;
const uint OFFSET_MASK         = (1u << BIT_PER_OFFSET) - 1u;
const uint COLOR_GRADIENT_MASK = (1u << BIT_PER_COLOR_GRADIENT) - 1u;
const uint WIND_GRADIENT_MASK  = (1u << BIT_PER_WIND_GRADIENT) - 1u;

void unpack_vertex_data(out ivec3 o_vox_local_pos, out uvec3 o_vert_offset_in_vox,
                        out float o_color_gradient, out float o_wind_gradient, uint packed_data) {
    // extract position bits and convert to signed coordinates
    uint pos_x = packed_data & POS_MASK;
    uint pos_y = (packed_data >> BIT_PER_POS) & POS_MASK;
    uint pos_z = (packed_data >> (BIT_PER_POS * 2)) & POS_MASK;

    const int OFFSET = 1 << (BIT_PER_POS - 1);
    o_vox_local_pos  = ivec3(pos_x, pos_y, pos_z) - OFFSET;

    // extract vertex offset within voxel
    uint offset_packed = (packed_data >> POS_BITS) & ((1u << OFFSET_BITS) - 1u);
    o_vert_offset_in_vox =
        uvec3(offset_packed & OFFSET_MASK, (offset_packed >> BIT_PER_OFFSET) & OFFSET_MASK,
              (offset_packed >> (BIT_PER_OFFSET * 2)) & OFFSET_MASK);

    // extract and normalize gradients
    uint gradient_data =
        (packed_data >> (POS_BITS + OFFSET_BITS)) & ((1u << BIT_PER_CUSTOM_INFO) - 1u);
    uint color_gradient_raw = gradient_data & COLOR_GRADIENT_MASK;
    uint wind_gradient_raw  = (gradient_data >> BIT_PER_COLOR_GRADIENT) & WIND_GRADIENT_MASK;

    o_color_gradient = float(color_gradient_raw) / float(COLOR_GRADIENT_MASK);
    o_wind_gradient  = float(wind_gradient_raw) / float(WIND_GRADIENT_MASK);
}

#endif // UNPACKER_GLSL
