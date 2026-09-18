import tree_scene_types;
[[vk::binding(50,0)]] RWStructuredBuffer<TreeSceneNode> tree_scene_nodes;
[[vk::binding(51,0)]] StructuredBuffer<uint4> tree_scene_primitives;
[[vk::binding(52,0)]] StructuredBuffer<float4> tree_scene_vertices;
[[vk::binding(63,0)]] StructuredBuffer<uint4> tree_refit_order;
struct RefitRange { uint offset; uint count; };
[[vk::push_constant]] ConstantBuffer<RefitRange> refit_range;

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= refit_range.count) return;
    uint4 record = tree_refit_order[refit_range.offset + id.x];
    TreeSceneNode node;
    node.escape = record.y;
    node.primitive = record.z;
    if (record.z != 0xffffffffu) {
        uint4 primitive = tree_scene_primitives[record.z];
        float3 a = tree_scene_vertices[primitive.x*2u].xyz;
        float3 b = tree_scene_vertices[primitive.y*2u].xyz;
        node.lower = min(a,b); node.upper = max(a,b);
        if (primitive.w == 0u) {
            float3 c = tree_scene_vertices[primitive.z*2u].xyz;
            node.lower = min(node.lower,c); node.upper = max(node.upper,c);
        }
    } else {
        TreeSceneNode left = tree_scene_nodes[record.x+1u];
        TreeSceneNode right = tree_scene_nodes[left.escape];
        node.lower = min(left.lower,right.lower);
        node.upper = max(left.upper,right.upper);
    }
    node.lower -= 1e-6; node.upper += 1e-6;
    tree_scene_nodes[record.x] = node;
}
