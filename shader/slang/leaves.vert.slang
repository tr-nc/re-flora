import leaf_model_display;
struct VertexInput {
    [[vk::location(0)]] uint packed_data:TEXCOORD0;
    uint instance_index:SV_VulkanInstanceID;
};
[shader("vertex")]
LeafModelVertex main(VertexInput input) {
    return displayAttachedLeaf(input.packed_data,input.instance_index);
}
