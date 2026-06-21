#ifndef FLORA_OCCUPANCY_GLSL
#define FLORA_OCCUPANCY_GLSL

// Occupancy image values are transient edit/build data. Zero means empty; non-zero
// values carry both the selected flora paint mode/species and growth progress.
const uint FLORA_OCCUPANCY_SELECTION_GRASS_MIX = 254u;
const uint FLORA_OCCUPANCY_SELECTION_NATURAL   = 255u;

const uint FLORA_OCCUPANCY_OCCUPIED_BIT      = 1u;
const uint FLORA_OCCUPANCY_GROWTH_SHIFT      = 1u;
const uint FLORA_OCCUPANCY_GROWTH_MASK       = 0xffu;
const uint FLORA_OCCUPANCY_SELECTION_SHIFT   = 9u;
const uint FLORA_OCCUPANCY_SELECTION_MASK    = 0xffu;

uint pack_flora_occupancy(uint selection, uint growth_progress) {
    return FLORA_OCCUPANCY_OCCUPIED_BIT |
           ((growth_progress & FLORA_OCCUPANCY_GROWTH_MASK) << FLORA_OCCUPANCY_GROWTH_SHIFT) |
           ((selection & FLORA_OCCUPANCY_SELECTION_MASK) << FLORA_OCCUPANCY_SELECTION_SHIFT);
}

uint unpack_flora_occupancy_growth(uint occupancy_value) {
    return (occupancy_value >> FLORA_OCCUPANCY_GROWTH_SHIFT) & FLORA_OCCUPANCY_GROWTH_MASK;
}

uint unpack_flora_occupancy_selection(uint occupancy_value) {
    return (occupancy_value >> FLORA_OCCUPANCY_SELECTION_SHIFT) & FLORA_OCCUPANCY_SELECTION_MASK;
}

#endif // FLORA_OCCUPANCY_GLSL
