module grass_visibility;
import flora_types;

// Layout is owned by grass_visibility.rs / FloraFramePlan. The five command
// words are Vulkan's indexed-indirect ABI; payload indices remain species-local.
[[vk::binding(10, 1)]] public StructuredBuffer<uint> grass_visible_instances;
public uint grassVisibleCount(PushConstantFlora pc) {
    return grass_visible_instances[pc.grass_visibility_offset + 1u];
}
public uint grassSourceIndex(PushConstantFlora pc, uint compactIndex) {
    return grass_visible_instances[pc.grass_visibility_offset + 5u + compactIndex];
}
