import grass_growth_shape;
import flora_types;

// Same geometry contract as flora::grass_acoustics::grass_shader_geometry_contract.
// Actual renderer height sampling + discrete growth, including stem/root offset.
export __extern_cpp int main()
{
    uint checksum = 0u;
    uint progressValues[5] = {0u, 32u, 64u, 128u, 255u};
    float potentials[3] = {0.0, 0.3, 1.0};
    for (uint x=0u; x<64u; ++x) {
        uint3 root = uint3(x*7u, x*3u+1u, x*11u);
        uint seed = instanceSeed(root + uint3(0u,1u,0u));
        for (uint species=0u; species<2u; ++species) {
            uint height = sampleGrassHeight(species,seed);
            for (uint p=0u; p<5u; ++p) {
                for (uint c=0u; c<3u; ++c) {
                    float grown = grassGrownHeight(height,min(float(progressValues[p])/255.0,potentials[c]));
                    checksum = checksum*31u + uint(grown);
                }
            }
        }
    }
    return checksum == 192770237u ? 0 : 1;
}
