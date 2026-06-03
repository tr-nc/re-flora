#ifndef FLORA_REGISTRY_GLSL
#define FLORA_REGISTRY_GLSL

const uint MAX_FLORA_SPECIES                 = 4;
const uint FLORA_SPECIES_COUNT               = 4;
const uint MAX_FLORA_INSTANCES_PER_SPECIES   = 40000;
const uint FLORA_SPECIES_TALL_GRASS  = 0;
const uint FLORA_SPECIES_SHORT_GRASS = 1;
const uint FLORA_SPECIES_LAVENDER    = 2;
const uint FLORA_SPECIES_EMBER_BLOOM = 3;
// Render-only tree instance types; not included in FLORA_SPECIES_COUNT.
const uint FLORA_SPECIES_TREE_LEAF   = 4;
const uint FLORA_SPECIES_APPLE       = 5;

uint flora_species_instance_offset(uint species_idx) {
    return species_idx * MAX_FLORA_INSTANCES_PER_SPECIES;
}

#endif // FLORA_REGISTRY_GLSL
