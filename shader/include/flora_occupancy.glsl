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
const uint FLORA_OCCUPANCY_SPAWN_AGE_SHIFT   = 17u;
const uint FLORA_OCCUPANCY_SPAWN_AGE_MASK    = 0x3fffu;
const uint FLORA_OCCUPANCY_SPAWN_INACTIVE    = FLORA_OCCUPANCY_SPAWN_AGE_MASK;

uint pack_flora_occupancy_with_spawn_age(uint selection, uint growth_progress,
                                         uint spawn_age_ms) {
    uint packed_spawn_age = min(spawn_age_ms, FLORA_OCCUPANCY_SPAWN_INACTIVE);
    return FLORA_OCCUPANCY_OCCUPIED_BIT |
           ((growth_progress & FLORA_OCCUPANCY_GROWTH_MASK) << FLORA_OCCUPANCY_GROWTH_SHIFT) |
           ((selection & FLORA_OCCUPANCY_SELECTION_MASK) << FLORA_OCCUPANCY_SELECTION_SHIFT) |
           ((packed_spawn_age & FLORA_OCCUPANCY_SPAWN_AGE_MASK)
            << FLORA_OCCUPANCY_SPAWN_AGE_SHIFT);
}

uint pack_flora_occupancy(uint selection, uint growth_progress) {
    return pack_flora_occupancy_with_spawn_age(selection, growth_progress,
                                               FLORA_OCCUPANCY_SPAWN_INACTIVE);
}

uint unpack_flora_occupancy_growth(uint occupancy_value) {
    return (occupancy_value >> FLORA_OCCUPANCY_GROWTH_SHIFT) & FLORA_OCCUPANCY_GROWTH_MASK;
}

uint unpack_flora_occupancy_selection(uint occupancy_value) {
    return (occupancy_value >> FLORA_OCCUPANCY_SELECTION_SHIFT) & FLORA_OCCUPANCY_SELECTION_MASK;
}

uint unpack_flora_occupancy_spawn_age_ms(uint occupancy_value) {
    return (occupancy_value >> FLORA_OCCUPANCY_SPAWN_AGE_SHIFT) &
           FLORA_OCCUPANCY_SPAWN_AGE_MASK;
}

#endif // FLORA_OCCUPANCY_GLSL
