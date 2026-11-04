module flora_light_sampling;
import light_reservoir;

// Stochastic identity belongs to the authored receiver, not the dispatch that
// happens to process it. Visibility compaction and GPU scheduling may reorder it.
public uint floraLightReceiverSeed(uint3 root, uint species, uint layer)
{
    return lightSamplingHash(root.x ^ lightSamplingHash(root.y + 17u) ^
        lightSamplingHash(root.z + 31u) ^ lightSamplingHash(species + 47u) ^
        lightSamplingHash(layer + 61u));
}
public uint2 floraLightProposalRandom(uint receiverSeed, uint proposal)
{
    uint state=lightSamplingHash(receiverSeed ^ 0x6a09e667u ^ lightSamplingHash(proposal));
    uint slot=lightSamplingHash(state) & 4095u;
    uint tap=min(uint(lightSamplingRandom(state)*12.0),11u);
    return uint2(slot,tap);
}
