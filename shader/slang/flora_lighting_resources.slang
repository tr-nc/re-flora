module flora_lighting_resources;

public [[vk::binding(15, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;
